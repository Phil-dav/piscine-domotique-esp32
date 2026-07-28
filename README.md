# Gestion de filtration piscine — ESP32 / Rust

Système domotique de pilotage d'une pompe de filtration de piscine, développé en
Rust sur ESP32 (esp-idf-hal / esp-idf-svc). Portage et évolution d'un projet
C++/PlatformIO de référence, avec plusieurs fonctionnalités de sécurité et de suivi
ajoutées en cours de route.

## Fonctionnalités

- **Trois modes de fonctionnement** (interrupteur physique 3 positions) : AUTO
  (filtration calculée selon la température de l'eau et une plage horaire),
  MANUEL (marche/arrêt piloté par bouton ou page web), OFF.
- **Sécurités matérielles** : niveau d'eau, défaut moteur (disjoncteur), anti-gel,
  canicule, anti-claquement du relais (protection contre un cyclage trop rapide).
- **Task Watchdog** : tout blocage de plus de 5 s de la boucle principale déclenche
  un redémarrage automatique et journalisé, plutôt qu'un blocage total silencieux.
- **Tableau de bord web embarqué** (servi directement par l'ESP32) : température
  eau/air, humidité, état pompe, mode, position GPS, tension batterie, timeline
  colorée des modes de la journée, boost/marche forcée, réglage de la plage
  horaire, consultation des journaux (sessions pompe, bilans journaliers, alertes).
- **Journalisation persistante** (NVS) : sessions pompe, bilans journaliers,
  alertes, et compteurs du jour — survivent aux redémarrages.
- **Suivi cloud en parallèle** vers [ThingSpeak](https://thingspeak.com) et
  [Adafruit IO](https://io.adafruit.com), pour consulter les courbes à distance
  sans dépendre d'un PC allumé en continu. Les appels réseau tournent dans des
  fils d'exécution dédiés, jamais dans la boucle principale surveillée par le
  watchdog.
- **Horodatage GPS avec repli NTP** : heure fiable même sans réseau Wi-Fi.
- **Suivi de l'alimentation batterie/solaire** (en cours de câblage) : deux ponts
  diviseurs de tension pour surveiller la batterie 12V et la sortie régulée 5V
  d'un contrôleur de charge solaire.

## Matériel

- ESP32 (DevKit classique)
- Capteur AHT10 (température/humidité air, I2C)
- Sonde DS18B20 (température eau, 1-Wire)
- Module GPS (UART) — horodatage et coordonnées
- PCF8574 (expandeur I2C) — relais pompe, relais défaut système, interrupteur de
  mode, bouton écran, entrée défaut moteur
- Écran OLED SSD1306 (I2C)
- Détecteur de niveau d'eau (ADC)
- Ponts diviseurs de tension pour le suivi batterie (ADC)

## Démarrage

1. Copier `src/config.example.rs` vers `src/config.rs` et y renseigner tes propres
   identifiants (Wi-Fi, clés ThingSpeak/Adafruit IO si tu utilises ces services —
   sinon laisse les valeurs par défaut, le programme continue de fonctionner sans
   Wi-Fi ni cloud).
2. Toolchain ESP-IDF `v5.5.3`, cible `xtensa-esp32-espidf` (voir
   `rust-toolchain.toml` et `.cargo/config.toml`).
3. `cargo check` pour vérifier, `cargo run` pour flasher (nécessite `espflash`).

## État du projet

En développement actif — les fonctionnalités listées ci-dessus sont opérationnelles
et testées en conditions réelles. Chantiers en cours ou à venir : carte SD pour un
historique complet, capteur de courant sur l'alimentation pompe, alarme de
température du coffret électrique, alertes push (ntfy.sh).

## Licence

Projet personnel, à but pédagogique et domestique.
