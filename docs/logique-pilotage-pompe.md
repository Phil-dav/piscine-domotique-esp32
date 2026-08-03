# Logique de pilotage de la pompe de filtration

Ce document explique **comment et pourquoi la pompe démarre ou s'arrête**, dans
le firmware ESP32 (`src/main.rs` et modules associés). Il ne reprend pas
l'architecture matérielle générale (voir le [README](../README.md) pour ça) —
uniquement la logique de décision marche/arrêt.

Le code ne raisonne pas comme "trois modes indépendants" : il empile plusieurs
niveaux de priorité, où les sécurités passent **au-dessus** du mode
sélectionné, quel qu'il soit. Ce document suit donc cet ordre, du plus
prioritaire (peut couper la pompe quoi qu'il arrive) au moins prioritaire
(logique propre à chaque mode).

## Vue d'ensemble : la pyramide de priorité

Du haut (prioritaire) vers le bas :

1. **Anti-claquement** (protection relais/moteur) — `pompe.rs`
2. **Sécurités process** (niveau d'eau, défaut moteur) — `niveau_eau.rs`, `securite.rs`
3. **Anti-gel absolu** (température < 2°C) — outrepasse même le mode OFF
4. **Mode sélectionné** (OFF / MANUEL / AUTO) — `main.rs`, `boost.rs`, `filtration_auto.rs`
5. **Application finale** au relais — `pompe.rs`

Chaque niveau peut imposer l'arrêt (ou, pour l'anti-gel, forcer la marche)
indépendamment de ce que demandent les niveaux en dessous.

## 1. Anti-claquement (`pompe.rs`)

Protection purement matérielle du relais et du moteur, indépendante de toute
logique métier. Si la pompe change d'état plus de **4 fois en 2 minutes**
(fenêtre glissante volontairement large, pour intercepter aussi un cyclage
plus lent — ex. lors d'une variation rapide de température pendant un orage),
elle est **bloquée de force pendant 5 minutes**, quel que soit le mode ou les
autres sécurités.

## 2. Sécurités process

Deux conditions forment `systeme_sur` (`niveau_ok && !securite_moteur.actif()`).
Si `systeme_sur` est `false`, la pompe est coupée immédiatement.

### Niveau d'eau bas (`niveau_eau.rs`)

Lecture ADC avec **hystérésis à deux seuils** (un seuil bas pour couper, un
seuil haut plus élevé pour réarmer — évite les allers-retours si le niveau
oscille pile sur un seul seuil) et un **filtre temporel anti-vaguelettes**
(3 à 5 secondes de confirmation avant de basculer l'état, pour ignorer un
remous passager sans réagir trop lentement à un vrai manque d'eau).

Cas particulier du tout premier relevé après un démarrage : appliqué
**immédiatement**, sans attendre le délai de confirmation — ni excès de
prudence (bloquer sans raison), ni excès de confiance (laisser tourner à sec
le temps que le filtre se stabilise). Voir le commentaire dans
`NiveauEau::mettre_a_jour`.

### Défaut moteur (`securite.rs`)

Contact du disjoncteur lu via le PCF8574 (contact fermé = normal, contact
ouvert = défaut). Anti-rebond de 200 ms avant confirmation. **Une fois
confirmé, le défaut reste verrouillé** même si le contact se referme tout
seul — un réarmement manuel explicite (page web) est nécessaire. Volontaire :
un défaut moteur ne doit jamais se lever tout seul sans qu'un humain ait
vérifié la cause.

## 3. Anti-gel absolu

Si la température de l'eau descend sous **2°C**, la pompe est forcée en
marche pour protéger la tuyauterie — **y compris en mode OFF**. C'est la
seule règle qui outrepasse le mode sélectionné lui-même (les sécurités des
niveaux 1 et 2 restent malgré tout prioritaires par-dessus).

## 4. Mode sélectionné

### Mode OFF

`demande_pompe = false` systématiquement, sauf l'anti-gel absolu ci-dessus.

### Mode MANUEL

`demande_pompe = demande_manuelle` — une simple variable pilotée par le
bouton physique ou la page web. Aucune logique automatique : marche/arrêt à
la demande, toujours soumis aux sécurités des niveaux 1 et 2.

### Mode AUTO

Le plus élaboré, avec plusieurs couches :

1. **Calcul de l'objectif d'heures du jour** (`filtration_auto.rs`, fonction
   `heures_cibles`), selon la température de l'eau, avec 5 régimes à
   hystérésis (pour éviter les oscillations aux seuils) :
   - **Anti-gel** (< 4°C) : 24h/24, plage horaire élargie à toute la journée.
   - **Canicule** (> 28,5°C) : 24h/24 aussi.
   - **Hiver** (< 9,5°C, sans atteindre l'anti-gel) : 2h fixes, plage 10h-16h.
   - **Normal** (10,5°C à 28,5°C) : formule température/2, +1h bonus si l'eau
     dépasse 24°C.
2. **Fiabilité de l'objectif** (`objectif_connu`) : si la sonde DS18B20 ne
   répond pas, pas de filtration AUTO plutôt que de filtrer « à l'aveugle ».
3. **Verrou journalier** (`filtration_terminee_aujourdhui`) : une fois
   l'objectif d'heures atteint, la pompe ne redémarre plus ce jour-là, même
   si l'objectif recalculé change ensuite (ex. chute de température lors
   d'un orage) — évite un cyclage marche/arrêt/marche.
4. **Plage horaire** (`dans_plage`) : la pompe ne démarre en AUTO que dans la
   fenêtre configurée (par défaut 8h-20h, sauf anti-gel/canicule qui
   l'étendent à 24h/24).
5. **Boost** (`boost.rs`) : disponible uniquement en AUTO, marche/arrêt
   forcée temporaire (30 min à 8h, réglable par paliers de 30 min) qui prend
   le pas sur toute la logique ci-dessus pendant sa durée, puis rend
   automatiquement la main.

En résumé : `demande_pompe` (AUTO) = `objectif_connu && !filtration_terminee_aujourdhui && dans_plage`
(ou l'inverse imposé par le boost s'il est actif).

## 5. Application finale (`pompe.rs`)

Quel que soit le mode, la `demande_pompe` calculée passe dans
`GestionPompe::mettre_a_jour(demande, systeme_sur)`, qui applique dans
l'ordre : blocage anti-claquement en cours → sécurité (`systeme_sur`) → et
seulement alors, applique réellement la demande et compte la transition pour
l'anti-claquement.
