# Automate de piscine — projet principal

Paquet : `pcf8574`
Carte : ESP32 classique (bicœur, Xtensa), cible `xtensa-esp32-espidf`.
Port série : **COM4**

## Ce que fait ce dossier

Le pilotage complet de la filtration : lecture des températures (eau et air),
calcul du temps de filtration nécessaire, commande du relais de pompe, sécurités
(niveau d'eau, défaut moteur, anti-gel, canicule), écran embarqué, horloge GPS,
tableau de bord web servi par la carte elle-même, et envoi vers Adafruit IO.

C'est le seul dossier en production : il tourne en permanence.

## Commandes

```
flash piscine      # téléverse (C:\rust\3, cargo run)
connect piscine    # capture le port série vers C:\rust\3\logs
```

Les journaux sont écrits en deux fichiers par capture :
`log_piscine_<date>_fonctionnement.txt` (lignes `I`) et `..._defauts.txt`
(lignes `W` et `E`).

## À savoir avant de toucher au code

**Incrémenter `VERSION_FIRMWARE` dans `src/config.rs` avant chaque flash.** C'est
le seul moyen de savoir, en lisant un journal, quelle version tournait.

**Un arrêt d'urgence câblé, indépendant de l'ESP32**, coupe l'alimentation du
relais de puissance. Les délais et sécurités logiciels relèvent donc du confort
de fonctionnement, pas de la sécurité des personnes.

`src/config.rs` contient les identifiants Wi-Fi et les clés Adafruit : il n'est
pas versionné. `config.example.rs` en tient lieu de modèle.

## Documentation

- `README.md` — installation, matériel, mise en route
- `docs/etat-projet.md` — état des lieux, réécrit à chaque bilan
- `docs/logique-pilotage-pompe.md` — la pyramide de priorité des décisions
- `docs/these-wifi-esp32*.md` — le récit de l'enquête sur l'instabilité réseau
- `.claude/skills/` — procédures de compilation, de diagnostic et de portage

## État au 16/08/2026

Version **1.9** en service. La **2.0** est compilée et vérifiée mais **pas encore
flashée** : elle corrige la reconnexion Wi-Fi, qui n'exécutait rien
(`disconnect()` manquant avant `connect()`), et espace le journal série de 2 s à
10 s avec déclenchement immédiat sur changement d'état.

Voir `c:\rust\5` — l'espion Wi-Fi, construit pour observer cette carte depuis
l'extérieur.
