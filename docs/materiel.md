# Matériel utilisé

Une fiche par composant du montage : rôle dans le projet, lien vers la fiche
technique (datasheet) officielle du fabricant, et photo/schéma quand disponible.

Voir aussi le [schéma électronique complet](schema-electronique.png) et le
[tableau d'architecture matérielle](../README.md#architecture-matérielle) dans le
README, plus synthétique.

## AHT10

**Rôle** : température et humidité de l'air (I2C, adresse 0x38).

**Fiche technique** : [AHT10 — datasheet (PDF)](https://components101.com/sites/default/files/component_datasheet/AHT10.pdf)

## PCF8574

**Rôle** : expandeur de broches I2C (adresse 0x21) — pilotage du relais pompe et
du relais de défaut système, lecture des boutons et de l'interrupteur de mode,
lecture du défaut moteur.

**Fiche technique** : [PCF8574 — datasheet officielle NXP (PDF)](https://www.nxp.com/docs/en/data-sheet/PCF8574_PCF8574A.pdf)

## SSD1306

**Rôle** : contrôleur de l'écran OLED de diagnostic (I2C, adresse 0x3C).

**Fiche technique** : [SSD1306 — datasheet officielle Solomon Systech (PDF)](https://cdn-shop.adafruit.com/datasheets/SSD1306.pdf)

## DS18B20

**Rôle** : sonde de température de l'eau (bus 1-Wire, GPIO4).

**Fiche technique** : [DS18B20 — page produit et datasheet officielle Analog Devices/Maxim](https://www.analog.com/en/products/ds18b20.html)

## Module GPS — GY-NEO6MV2 (puce u-blox NEO-6M)

**Rôle** : horodatage et coordonnées GPS (UART2, GPIO16 RX).

Carte de rupture (breakout board) générique GY-NEO6MV2/GY-GPS6MV2, basée sur la
puce u-blox NEO-6M — existe en variante 4 ou 5 broches (la 5ᵉ broche ajoute
généralement une sortie PPS, non utilisée dans ce projet).

**Fiche technique** : [NEO-6 — datasheet officielle u-blox (PDF)](https://content.u-blox.com/sites/default/files/products/documents/NEO-6_DataSheet_(GPS.G6-HW-09005).pdf)

## Détecteur de niveau d'eau — poire à flotteur, contact sec

**Rôle** : sécurité niveau d'eau (ADC1, GPIO34).

Poire de niveau à flotteur, contact sec NO/NC (normalement ouvert/fermé) — composant
purement mécanique, pas d'électronique embarquée. Contact ouvert/fermé lu côté
firmware via une lecture ADC avec seuils et hystérésis (`niveau_eau.rs`), le même
principe s'applique quel que soit le modèle exact choisi. Pas de référence précise
retenue pour l'instant — n'importe quelle poire générique à contact sec NO/NC fait
l'affaire (ex. modèles disponibles chez ManoMano, Assainipompes.fr,
Pompes-direct.com).

**Fiche technique** : composant mécanique générique, pas de datasheet dédiée
nécessaire — juste vérifier le type de contact (NO/NC) et le courant max supporté
(5A largement suffisant, le contact ne pilote qu'une entrée logique).

## PH-4502C

**Rôle** : mesure du pH de l'eau — module **amovible**, pas branché en permanence
(voir la mémoire du projet : vérification ponctuelle sous tension, résistance de
tirage 200 kΩ-1 MΩ prévue sur l'ADC pour détecter branchement/débranchement). Déjà
en main (testé avec Arduino Uno), câblage sur l'ESP32 et mise à jour du schéma
électronique restants à faire.

Alimentation 5±0,2V, sortie analogique 0-5V (pont diviseur nécessaire vers l'ADC
ESP32, limité à 3,3V), électrode à connecteur BNC, 2 potentiomètres de calibration
à bord. Temps de réponse ≤ 5s, stabilisation ≤ 60s.

**Fiche technique** : [page produit officielle diymore.cc](https://www.diymore.cc/products/diymore-liquid-ph-value-detection-detect-sensor-module-monitoring-control-for-arduino-m) —
voir aussi ce [guide d'utilisation et de calibration détaillé](https://raaflahar.medium.com/ph-4502c-sensor-diymore-how-to-use-and-calibrate-using-arduino-uno-r3-3afc2b96631).

**Photo** : test avec Arduino Uno, voir `c:\rust\Images vidéos\PH metre arduino.jpeg`.
