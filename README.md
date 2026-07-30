# Gestion de filtration piscine — ESP32 / Rust

![Rust](https://img.shields.io/badge/Rust-esp--idf--hal-orange)
![Plateforme](https://img.shields.io/badge/plateforme-ESP32-blue)
![Licence](https://img.shields.io/badge/licence-projet%20personnel-lightgrey)

Système domotique embarqué qui pilote la pompe de filtration d'une piscine
familiale : température de l'eau, sécurités matérielles, planification
automatique, tableau de bord web, suivi cloud. Écrit en Rust sur ESP32
(`esp-idf-hal` / `esp-idf-svc`), portage et évolution d'un projet
C++/PlatformIO de référence.

## Sommaire

- [Aperçu](#aperçu)
- [Fonctionnalités](#fonctionnalités)
- [Architecture matérielle](#architecture-matérielle)
- [Démarrage](#démarrage)
- [Tableau de bord web](#tableau-de-bord-web)
- [État du projet](#état-du-projet)
- [Licence](#licence)

## Aperçu

Le montage pilote une pompe de filtration selon trois modes sélectionnables
par un interrupteur physique — **AUTO** (durée de filtration calculée selon la
température de l'eau, dans une plage horaire configurable), **MANUEL**
(marche/arrêt à la demande) et **OFF** — avec un jeu de sécurités matérielles
(niveau d'eau, défaut moteur, anti-gel, canicule) qui prennent le pas sur
n'importe quel mode en cas de besoin. Un serveur web embarqué directement sur
l'ESP32 sert un tableau de bord en temps réel, sans dépendance à un service
externe pour fonctionner au quotidien.

## Fonctionnalités

- **Trois modes de fonctionnement** (interrupteur physique 3 positions) : AUTO,
  MANUEL (bouton ou page web), OFF.
- **Sécurités matérielles** : niveau d'eau, défaut moteur (disjoncteur),
  anti-gel, canicule, anti-claquement du relais (protection contre un cyclage
  trop rapide).
- **Task Watchdog** : tout blocage de plus de 5 s de la boucle principale
  déclenche un redémarrage automatique et journalisé, plutôt qu'un blocage
  total silencieux.
- **Tableau de bord web embarqué**, servi directement par l'ESP32 : température
  eau/air, humidité, état pompe, mode, position GPS, tension batterie, timeline
  colorée des modes de la journée (avec trace réelle de la marche pompe),
  marche forcée, réglage de la plage horaire, consultation des journaux
  (sessions pompe, bilans journaliers, alertes).
- **Journalisation persistante** (NVS) : sessions pompe, bilans journaliers,
  alertes, et compteurs du jour — survivent aux redémarrages.
- **Suivi cloud** vers [Adafruit IO](https://io.adafruit.com) : envoi groupé de
  toutes les mesures en une seule requête HTTP par cycle, dans un fil
  d'exécution dédié, jamais dans la boucle principale surveillée par le
  watchdog.
- **Horodatage GPS avec repli NTP** : heure fiable même sans réseau Wi-Fi.
- **Suivi de l'alimentation batterie/solaire** (en cours de câblage) : deux
  ponts diviseurs de tension pour surveiller la batterie 12V et la sortie
  régulée 5V d'un contrôleur de charge solaire.

## Architecture matérielle

| Bus / broche | Composant | Rôle |
| --- | --- | --- |
| I2C (GPIO21 SDA, GPIO22 SCL, 400 kHz) | AHT10 (0x38) | Température / humidité air |
| I2C | PCF8574 (0x21) | Expandeur de broches (relais, boutons, interrupteur de mode) |
| I2C | SSD1306 (0x3C) | Écran OLED de diagnostic |
| 1-Wire (GPIO4) | DS18B20 | Température de l'eau |
| UART2 (GPIO16 RX) | Module GPS | Horodatage et coordonnées |
| ADC1 (GPIO34) | Détecteur de niveau | Sécurité niveau d'eau |
| ADC1 (GPIO32, GPIO35) | Ponts diviseurs | Tension batterie 12V / sortie solaire 5V |

Détail du câblage du PCF8574 : P0 pilote le relais pompe, P3 le relais de
défaut système, P4/P5/P6 lisent respectivement le bouton écran et
l'interrupteur de mode (MANU/AUTO), P7 lit le retour de défaut moteur.

## Démarrage

1. Copier `src/config.example.rs` vers `src/config.rs` et y renseigner tes
   propres identifiants (Wi-Fi, clé Adafruit IO si tu utilises ce service —
   sinon laisse les valeurs par défaut, le programme continue de fonctionner
   sans Wi-Fi ni cloud).
2. Toolchain ESP-IDF `v5.5.3`, cible `xtensa-esp32-espidf` (voir
   `rust-toolchain.toml` et `.cargo/config.toml`).
3. `cargo check` pour vérifier, `cargo run` pour flasher (nécessite
   `espflash`).

## Tableau de bord web

Servi directement par l'ESP32 sur le réseau local (pas de service externe
requis) : mesures en temps réel, contrôle de la pompe et du mode, timeline
colorée de la journée, historique des sessions/alertes, réglage de la plage
de filtration.

Captures d'écran à ajouter.

## État du projet

En développement actif — les fonctionnalités listées ci-dessus sont
opérationnelles et testées en conditions réelles. Chantiers en cours ou à
venir : carte SD pour un historique complet, capteur de courant sur
l'alimentation pompe, alarme de température du coffret électrique, alertes
push (ntfy.sh).

## Licence

Projet personnel, à but pédagogique et domestique.
