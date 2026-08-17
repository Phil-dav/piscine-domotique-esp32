# Bilan de consommation électrique

> Estimations basées sur les datasheets officiels des composants (sources
> citées pour chaque valeur), pas des mesures physiques. À confirmer un jour
> au multimètre/pince ampèremétrique si une vérification réelle est utile.

## Contexte

Suite à la conversion du PCB en 3,3V pour tout sauf le VIN de l'ESP32 et le
module 4 relais (voir la mémoire du projet sur le sujet), ce document répond à
la question : le régulateur 3,3V embarqué de l'ESP32 a-t-il assez de marge
pour alimenter tous les nouveaux composants raccordés dessus (écran OLED,
GPS, AHT10, PCF8574 et ses résistances/LED) ?

**Point important** : le rail 3,3V n'est pas une alimentation indépendante —
il est produit par le régulateur embarqué de l'ESP32 à partir du 5V d'entrée
(VIN). Le courant du rail 3,3V se retrouve donc intégralement dans le courant
tiré sur le 5V d'entrée (régulateur linéaire : courant d'entrée ≈ courant de
sortie, l'écart de tension est juste dissipé en chaleur). Le total réel à la
source d'alimentation 5V externe est donc : **module relais (direct 5V) +
courant du rail 3,3V (via le régulateur)** — pas une simple addition 5V + 3,3V
en parallèle.

## Rail 3,3V (via le régulateur embarqué de l'ESP32)

| Composant | Consommation | Source |
|---|---|---|
| ESP32-WROOM-32 (le module lui-même) | ~80-100 mA en fonctionnement normal, jusqu'à **240 mA en pointe** (émission Wi-Fi) | [Datasheet ESP32-WROOM-32](https://www.mouser.com/datasheet/2/891/esp-wroom-32_datasheet_en-1223836.pdf) |
| Module GPS GY-NEO6MV2 (NEO-6M) | ~45 mA en poursuite (jusqu'à ~67 mA au démarrage à froid) | [datasheethub.com](https://www.datasheethub.com/gy-neo6mv2-flight-control-gps-module/) |
| Écran OLED SSD1306 128×64 | ~20-25 mA typique (jusqu'à ~50 mA si affichage presque entièrement allumé) | [Datasheet SSD1306 (Adafruit)](https://cdn-shop.adafruit.com/datasheets/SSD1306.pdf) |
| AHT10 | ~1 mA max en mesure (négligeable, µA au repos) | [Datasheet AOSONG/ASAIR](https://server4.eca.ir/eshop/AHT10/Aosong_AHT10_en_draft_0c.pdf) |
| PCF8574 (puce elle-même) | ~10 µA en veille (négligeable) | [Datasheet TI](https://www.ti.com/product/PCF8574) |
| 4 résistances de tirage P4-P7 (10 kΩ vers 3,3V) | 0,33 mA chacune → **1,3 mA** au total | Calcul (loi d'Ohm, valeurs confirmées dans le schéma KiCad) |
| 4 LED D1-D4 (résistances série R1-R4 = 330 Ω) toutes allumées ensemble | ~4 mA chacune → **~16 mA** au total | Calcul (loi d'Ohm, seuil LED estimé ~2V) |

**Sous-total rail 3,3V** : ~190 mA en fonctionnement normal, jusqu'à **~330 mA en pointe** (Wi-Fi + GPS + écran chargé simultanément).

## Rail 5V direct (ne passe pas par le régulateur 3,3V)

| Composant | Consommation | Source |
|---|---|---|
| Module 4 relais (Songle SRD-05VDC-SL-C), les 4 énergisés en même temps | 71,4 mA par relais → **~285 mA** au total | [Datasheet SRD-05VDC-SL-C](https://www.circuitbasics.com/wp-content/uploads/2015/11/SRD-05VDC-SL-C-Datasheet.pdf) |

En pratique, il est rare que les 4 relais soient activés exactement en même
temps (pompe + ventilation coffret sont les deux usages prévus à ce jour) —
cette valeur est un plafond, pas la consommation habituelle.

## Total à la source d'alimentation 5V externe

| Scénario | Calcul | Total |
|---|---|---|
| Fonctionnement normal | 3,3V (~190 mA, via régulateur) + relais (2 relais actifs, ~140 mA) | **~330 mA** |
| Pire cas (tout en pointe en même temps) | 3,3V (~330 mA) + relais (4 actifs, ~285 mA) | **~615 mA** |

## Marge disponible

Le régulateur 3,3V embarqué de l'ESP32 (selon le modèle de carte, à vérifier
par le marquage sur le petit boîtier près du connecteur USB) :

- **AMS1117-3.3** (le plus courant) : 1 A max → la pointe de ~330 mA sur le
  rail 3,3V reste à **33% de sa capacité**, largement confortable.
- **ME6211C33** (SOT23-5, sur certains clones plus compacts) : 500 mA max →
  ~330 mA en pointe représente **66% de sa capacité** — toujours dans les
  clous, marge plus serrée.

**Conclusion (analysée le 04/08/2026)** : marge confortable dans les deux cas
pour la configuration actuelle. Pas besoin d'une alimentation 3,3V externe
séparée pour l'instant. À revérifier si un composant supplémentaire important
est ajouté un jour (le pH-mètre PH-4502C, par exemple, consomme lui-même très
peu — quelques mA — donc ne changerait pas cette conclusion).

Pensez aussi à vérifier que votre bloc d'alimentation 5V externe (celui qui
alimente le J5 "Alimentation 5V") est bien dimensionné pour au moins ~700 mA
avec de la marge, pour couvrir le pire cas ci-dessus sans être à sa limite.
