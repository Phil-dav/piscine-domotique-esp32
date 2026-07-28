---
name: diagnostic-plantage
description: Diagnostiquer un plantage/redémarrage inattendu du firmware ESP32 piscine (c:\rust\3) — dashboard inaccessible, écran figé, ou entrées suspectes dans le journal des alertes. À utiliser dès que l'utilisateur signale que la carte ne répond plus, ou pour analyser le journal après un incident.
---

# Diagnostiquer un plantage du firmware piscine

## Contexte : pourquoi un plantage est grave ici

Tout le programme (écran OLED, Wi-Fi, serveur web, pilotage pompe, journaux) tourne
dans **une seule boucle, une seule tâche** (`main()` dans `src/main.rs`). Un plantage
n'importe où arrête donc tout d'un coup : plus d'écran, plus de dashboard, plus de
pompe pilotée. C'est le symptôme typique rapporté par l'utilisateur : dashboard
inaccessible + écran qui ne réagit plus au bouton, en même temps.

**Premier réflexe pour confirmer un vrai plantage (pas juste un souci d'écran) :**
tester le dashboard web. S'il répond, ce n'est qu'un problème localisé à l'OLED
(la logique d'affichage est enveloppée dans un bloc qui attrape ses erreurs sans
arrêter le programme, voir le commentaire `resultat_ecran` dans `main.rs`) — pas un
vrai plantage.

## Étape 1 — Lire le journal des alertes en premier

`GET /log/alertes` (ou le bouton "Alertes" du dashboard) donne des indices même sans
accès au port série. Voir `src/journal.rs` pour le format et les limites (25 dernières
entrées seulement, borné en NVS).

**Repère clé : l'entrée `REDEMARRAGE`.** Depuis l'ajout de la détection de raison de
reset (`ResetReason::get()` dans `main.rs`, capturée tout au début de `main()`), chaque
démarrage journalise automatiquement :
- la raison ESP-IDF (`ALIMENTATION`, `SOUS_TENSION` (brownout), `PANIC`,
  `WATCHDOG_TACHE`, `WATCHDOG_INTERRUPTION`, `BOUTON_RESET`, `LOGICIEL`...)
- une estimation de la durée d'arrêt (comparaison avec le dernier horodatage
  sauvegardé en NVS toutes les 60 s — pas d'horloge qui tourne hors tension sur cet
  ESP32, donc c'est la seule façon d'estimer une durée de coupure)
- une classification "coupure courte" (< 2 min, seuil `SEUIL_COUPURE_COURTE_MIN` dans
  `main.rs`, choisi par l'utilisateur comme probable micro-coupure secteur EDF) vs
  "arrêt prolongé" (probablement volontaire, sans intérêt à investiguer)

**Avant cet ajout**, seule preuve indirecte de redémarrages répétés : l'alerte
`FEEDBACK_ROMPU` (fil de feedback pompe rompu, relais miroir sur GPIO33) ne se
réarmait **jamais** dans le code — donc si elle réapparaissait plusieurs fois dans
le journal, c'était la preuve d'un redémarrage. **Cette logique de feedback a été
retirée** (relais miroir jugé peu fiable, voir mémoire projet — remplacement prévu
par un capteur de courant) : `FEEDBACK_ROMPU` n'existe plus dans le code actuel,
cette astuce ne s'applique donc qu'aux journaux antérieurs à son retrait. Même
logique (toujours valable) pour toute autre alerte à front montant qui se répète
anormalement souvent.

## Étape 2 — Surveiller le port série en direct

Un seul processus peut tenir le port série (COM4 habituellement) à la fois sur
Windows — ouvrir un second `espflash monitor` en parallèle donne "Accès refusé" et
casse la capture en cours. Toujours vérifier qu'aucun autre moniteur ne tourne avant
d'en lancer un.

Pour capturer un plantage en direct sans polluer le contexte de conversation avec des
heures de logs : lancer `cargo run` en arrière-plan (voir skill `build-flash`), puis
un `Monitor` (grep sur les patterns `panic|Panic|Guru Meditation|Brownout|brownout|
watchdog|Watchdog|WDT|rst:|reboot|reset reason|abort\(\)|LoadProhibited|
StoreProhibited|error:|Error:|failed`) sur le fichier de sortie du processus en
arrière-plan.

**Piège horaire** : les timestamps `espflash`/ESP-IDF bruts (`[...T05:16:59Z...]`)
sont en **UTC**. Les logs applicatifs (`Heure : .../...`) sont déjà en heure locale
Europe/Paris (voir `src/temps.rs`, DST géré automatiquement). Ne pas comparer les deux
sans convertir.

## Suspects classés par plausibilité (aucun confirmé à ce jour)

| # | Piste | Indice |
|---|---|---|
| 1 | Brownout (chute de tension) | Aucune config brownout dans `sdkconfig.defaults` ; corrélation observée entre alertes `CANICULE` (pompe forcée 24h/24, appel de courant soutenu) et `FEEDBACK_ROMPU` ~30s après |
| 2 | Watchdog (tâche IDLE affamée) | Lecture I2C/1-Wire bloquante sans time-out (glitch bus) empêcherait la boucle d'atteindre le `FreeRtos::delay_ms(20)` final |
| 3 | Épuisement mémoire après longue durée | Serveur web sollicité en continu sur plusieurs heures/jours |
| 4 | Mutex empoisonné (effet domino) | `etat.lock().unwrap()` / `journal.lock().unwrap()` utilisés ~20 fois entre boucle principale et routes web — un seul panic n'importe où rend tous les appels suivants fatals |

Le champ `REDEMARRAGE` de l'étape 1 devrait maintenant trancher directement entre les
pistes 1/2 (matérielles, "SOUS_TENSION"/"WATCHDOG_*") et 4 ("PANIC").

## Bruit de fond connu (pas un plantage)

Les warnings `DS18B20 : erreur lecture donnée (ignorée) : CrcMismatch` sont gérés
proprement (ignorés, dernière valeur conservée) — bruit électrique sur le bus 1-Wire,
pas une cause de plantage en soi.

En revanche, `DS18B20 : erreur recherche 1-Wire : UnexpectedResponse` **n'est plus du
bruit anodin** : c'était la cause de décrochages durables de la sonde d'eau (`Eau :
NaN °C`, `cible:0.0h`, donc plus de filtration AUTO faute d'objectif calculable).
Corrigé le 26/07/2026 en mémorisant l'adresse du capteur au lieu de réénumérer le bus
à chaque lecture (`ds18b20.rs`), avec relance automatique du bus après 5 échecs
consécutifs et alerte `SONDE_EAU_MUETTE` au-delà de 5 minutes sans lecture valide. Si
ce warning réapparaît en rafale malgré ça, la piste devient matérielle (résistance de
tirage, câble de la sonde, parasites du relais pompe).

**Symptôme à connaître** : une sonde d'eau muette se traduit par une pompe qui ne
démarre jamais en AUTO alors que tout paraît normal au dashboard (aucun défaut, mode
AUTO, dans la plage horaire) — vérifier `Eau :` et `cible:` dans le journal série
avant de chercher ailleurs.

## Ne pas oublier

Toute modification touchant à la logique de pilotage du relais pompe demande de la
prudence (relire le code existant, expliquer les changements avant d'agir), mais le
mot-clé « sécurité-pompe » n'est **plus exigé** comme autorisation préalable — voir
skill `port-depuis-cpp`.
