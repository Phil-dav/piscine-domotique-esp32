---
name: port-depuis-cpp
description: Porter une fonctionnalité métier manquante (pompe, boost, anti-gel, canicule, planification, logs, GPS...) depuis le projet C++ de référence vers le firmware Rust ESP32 piscine. À utiliser dès que l'utilisateur demande d'implémenter une route serveur manquante ou un comportement du dashboard qui n'est pas encore branché côté Rust.
---

# Porter une fonctionnalité depuis le projet C++ de référence

Le firmware Rust `c:\rust\3` est un portage en cours du projet C++/PlatformIO/Arduino
`d:\Projet VScode\Gestion-filtration-piscine-ESP32`. Le dashboard web (`src/index.html`,
`src/script.js`) a été repris tel quel depuis ce projet C++ ; la logique métier n'a, elle,
pas encore été portée. Ne pas redéfinir cette logique depuis zéro : elle existe déjà,
réfléchie et documentée, côté C++.

## Étape 1 — Identifier le manager C++ de référence

Selon la fonctionnalité demandée, lire le module correspondant dans
`d:\Projet VScode\Gestion-filtration-piscine-ESP32\lib\` :

| Fonctionnalité | Manager C++ |
|---|---|
| Marche/arrêt pompe, boost, marche forcée | `PumpManager` |
| Sécurités, défauts moteur, verrouillage | `SafetyManager` |
| Mode Auto/Manuel/Off | `ModeManager` |
| Calcul durée de filtration selon T° eau, modes Gel/Canicule | `WaterTempManager` |
| Niveau d'eau | `WaterLevel` |
| Persistance (planification, durée boost...) | `StorageManager` |
| Journaux (sessions, bilans, alertes) | `LogManager` |
| Historique des modes | `ModeHistory` |
| GPS | `GPS_manager` |

Lire aussi `docs/journal-modifications.md` dans ce projet C++ pour le raisonnement
derrière les choix (ex. seuils, séquences de sécurité) — ne pas se contenter du code brut.

## Étape 2 — Repérer ce que le front-end Rust attend déjà

Le JSON `/sensors` (`c:\rust\3\src\web_server.rs`) et `EtatCapteurs`
(`c:\rust\3\src\etat_partage.rs`) définissent déjà les champs attendus par `script.js`.
Vérifier les routes appelées côté JS (`fetch('/...')` dans `script.js`) pour connaître
le contrat d'API exact à respecter (méthode, paramètres de requête, format de réponse).

## Étape 3 — Porter en respectant les conventions Rust du projet

- Un module par responsabilité dans `src/` (garder l'esprit "un manager = un fichier"
  du projet C++, adapté à l'idiome Rust).
- Pas de `delay()` bloquant dans la boucle principale au-delà de ce qui existe déjà
  (`FreeRtos::delay_ms` en fin de boucle) — utiliser des compteurs/timestamps pour toute
  temporisation métier (boost restant, planification), comme le fait le C++ avec `millis()`.
- Erreurs gérées via `anyhow::Result`, jamais de `panic!`/`unwrap()` sur une erreur matérielle
  récupérable (voir le style déjà en place dans `ds18b20.rs`, `aht10.rs`).

## Règle de prudence — pilotage du relais pompe

**Toute modification touchant à la logique de pilotage du relais pompe** (démarrage/arrêt
de la pompe, ou son équivalent Rust du `PumpManager`) comporte un risque de dommage matériel
(moteur, contacts du relais) en cas d'erreur.

Le mot-clé « sécurité-pompe » n'est **plus exigé** comme autorisation préalable de
l'utilisateur (retiré le 25/07/2026, à sa demande — il ne veut pas avoir à le répéter à
chaque fois). En revanche, la vigilance qu'il impliquait reste de mise : avant toute
modification de ce périmètre, relire attentivement le code existant, vérifier les
conséquences (anti-claquement, feedback, anti-gel absolu...), et rappeler clairement à
l'utilisateur ce qui va changer avant d'agir — sans bloquer sur un mot-clé précis.
