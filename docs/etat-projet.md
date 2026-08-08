# État du projet — point du 08/08/2026

> Ce fichier est mis à jour automatiquement à chaque fois que je fais un point
> avec l'utilisateur sur l'avancement du projet. Le contenu ci-dessous est
> remplacé intégralement à chaque nouveau point, ce n'est pas un historique.

## La journée du 08/08 : enquête Wi-Fi complète

Toute la session a été consacrée à une panne réseau, puis à sa cause racine.
Trois versions de firmware ont été flashées dans la journée (1.1, 1.2, 1.3).

### La panne constatée

À 09h48, le signal chute à -77 dBm. Suivent **1h30 de dashboard totalement
injoignable**, avec 5 réinitialisations complètes du pilote Wi-Fi qui ne
rétablissent rien. La Livebox voyait pourtant la carte comme connectée, avec
son IP. Seul un **reset manuel à 11h29** a rétabli le service.

Point important : ce n'était **pas** un plantage du firmware. Aucun
redémarrage, aucun panic, RAM stable (142-163 Ko), capteurs actifs pendant
tout l'épisode.

### Cause racine n°1 — le contrôle de santé Wi-Fi était trop faible

`is_connected()` d'esp-idf-svc ne reflète **que l'association 802.11** : un
simple drapeau alimenté par les événements du pilote. Il ne prouve à aucun
moment qu'un paquet circule.

Le piège : la logique de reconnexion de `main.rs` est gardée par
`if wifi_connecte { ... } else { ...reconnexion... }`. Tant que
`is_connected()` répond `true`, **aucun mécanisme de secours ne peut se
déclencher**. La carte pouvait donc rester indéfiniment « associée mais
morte », ce qui est exactement ce qui s'est produit.

Corrigé en 1.2 :

- contrôle de santé passé à `is_up()` **plus** vérification qu'une IP est
  réellement attribuée ;
- nouveau module `surveillance_reseau.rs` : sonde active qui ping la
  passerelle toutes les 60 s, dans un fil dédié (jamais dans la boucle
  principale, voir le piège des appels réseau bloquants) ;
- escalade en deux temps : réinitialisation du pilote après 5 min sans
  joignabilité, `esp_restart()` après 15 min, avec alerte `RESEAU_ZOMBIE`
  journalisée ;
- garde-fou anti-boucle : l'escalade exige qu'**au moins un ping ait réussi
  depuis le démarrage**. Une passerelle qui ignore l'ICMP ne peut donc jamais
  provoquer de redémarrages en cascade.

### Cause racine n°2 — la veille modem, jamais désactivée

Découverte par la mesure, pas par la lecture du code. Aucun réglage
d'économie d'énergie n'existait dans le projet : la carte utilisait donc le
défaut ESP-IDF en mode station, `WIFI_PS_MIN_MODEM`.

Corrigé en 1.3 : `esp_wifi_set_ps(WIFI_PS_NONE)` dans `wifi.rs`, appelé au
démarrage **et réappliqué après chaque `stop()`/`start()` du pilote**.
Application confirmée par le journal de boot (`wifi:Set ps type: 0`).

**Résultat mesuré, test A/B correctement contrôlé** — même matériel, même
signal, même charge (2 dashboards ouverts, ~1 requête/s), une seule ligne de
code d'écart :

| En charge | v1.2 | v1.3 |
| --- | --- | --- |
| Ping — perte de paquets | 75 % | **0 %** |
| Ping — latence moyenne | 1 627 ms | **9 ms** |
| HTTP — pire cas | échec à 20 s, puis 7,38 s | **0,131 s** |

À vide également : ping moyen 128 ms → 9 ms, et surtout disparition de la
gigue (45-253 ms → 6-12 ms).

### Correctif annexe côté navigateur

`fetch('/sensors')` n'avait aucun délai maximal. Une requête restée en
suspens bloquait définitivement le verrou `isFetching`, figeant le dashboard
jusqu'à un rechargement manuel. Ajout d'un `AbortController` à 8 s dans
`script.js`. Ce n'était pas la cause de la panne, seulement une fragilité
réelle.

## Ce qui reste ouvert

- **La panne « associée mais injoignable » n'est pas démontrée résolue.** Elle
  ne s'est pas reproduite depuis, donc la sonde et le redémarrage automatique
  n'ont jamais eu l'occasion de se déclencher en conditions réelles. La cause
  racine n°2 est très probablement la même, mais ça reste à confirmer sur
  plusieurs jours.
- **Budget de sockets très serré** (analysé, non corrigé) :
  `CONFIG_LWIP_MAX_SOCKETS = 10` pour tout le programme, dont 7 consommés par
  le seul serveur web (4 clients + 3 internes), plus NTP, Adafruit IO et la
  nouvelle sonde ping. On est **au plafond en pointe**. Par ailleurs
  `esp-idf-svc` limite à 4 connexions clients là où ESP-IDF lui-même utilise
  7 par défaut, alors qu'un navigateur en ouvre jusqu'à 6 en parallèle.
  Proposé pour une 1.4 : `CONFIG_LWIP_MAX_SOCKETS=16` et
  `max_open_sockets: 7`.

## À faire, par ordre de proximité

**Suite immédiate** :

- Laisser tourner la 1.3 plusieurs jours et surveiller le retour éventuel de
  l'indisponibilité longue.
- Décider si on applique la 1.4 (sockets) — moins urgent depuis que la
  latence est retombée à 90 ms.

**Chantier en pause** :

- Provisioning Wi-Fi (portail captif) — plus urgent depuis l'abandon du
  dashboard simplifié, reste utile pour la cave.

**Chantiers matériels, rien codé, câblage à faire** :

- Capteur de courant TA12-100 pour la pompe (remplace le relais miroir) — la
  pince de mesure de courant manque toujours pour calibrer avant de câbler.
- Ventilation active du coffret (relais P1 libre, vérif via INA219) — le
  coffret physique n'existe pas encore.
- Carte micro SD (broches confirmées CS=5/CLK=18/MISO=19/MOSI=23).
- Interrupteur logs série (GPIO27, schéma KiCad finalisé) — code de lecture
  GPIO27 pas encore écrit.
- pH-mètre (PH-4502C) — pont diviseur 5V→3,3V + résistance de tirage à
  monter, code de détection présence/absence à écrire.
- Conversion 3,3V du PCB (plan validé le 04/08, pas encore câblé).
- Alimentation batterie/solaire (idée, pas décidée) — à noter : la
  désactivation de la veille modem augmente la consommation moyenne, à
  reconsidérer si ce chantier démarre.

**Discord / présentation** :

- Synthèse narrative de l'enquête Wi-Fi à écrire pour le Discord, une fois le
  problème considéré résolu à ~100 % — avec les fausses pistes assumées.

**Idée non décidée, juste en mémoire** :

- Traduction anglaise de la doc à terme — pas de date.
