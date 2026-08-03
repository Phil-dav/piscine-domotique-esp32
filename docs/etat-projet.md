# État du projet — point du 30/07/2026

> Ce fichier est mis à jour automatiquement à chaque fois que je fais un point
> avec l'utilisateur sur l'avancement du projet. Le contenu ci-dessous est
> remplacé intégralement à chaque nouveau point, ce n'est pas un historique.

## Ce qui vient d'être fait (session en cours)

- **Cloud Adafruit IO** : envoi groupé (1 requête HTTP au lieu de 7), périodique
  + événementiel combinés pour pompe/mode (graphe continu + réactif), échelle
  2/3/4 sans chevauchement avec la pompe (0/1). ThingSpeak entièrement retiré.
- **Dashboard Adafruit IO** : légende avec flèches Unicode, bloc Indicator
  envisagé puis écarté (binaire seulement), solution retenue = bloc Text
  statique en légende par-dessus le graphique.
- **Timeline dashboard local** : bande pompe réelle + bande mode, persistance
  NVS, label heure sur curseur, marqueurs de transition avec tooltip — tout
  confirmé fonctionnel en conditions réelles.
- **4 corrections firmware** (compilées, clippy et fmt propres, **flashées**) :
  1. Retry Adafruit IO (3 tentatives, 2 s d'écart) sur échec réseau transitoire.
  2. Date de session pompe = date de **début**, plus de fin (corrige
     l'ambiguïté minuit).
  3. Garde-fou premier tour sur toutes les alertes (évite les doublons
     NIVEAU_EAU/DEFAUT_MOTEUR/ANTI_GEL/CANICULE/etc. au redémarrage à chaud).
  4. Niveau d'eau : première lecture après démarrage appliquée immédiatement,
     sans délai de confirmation (évite la fenêtre de ~3 s où un manque d'eau
     réel serait ignoré).
- **Documentation** : `docs/logique-pilotage-pompe.md` créé (pyramide de
  priorité du pilotage pompe), README avec logo/schéma/arbre de fichiers/
  capture du dashboard. Dépôt public `piscine-domotique-esp32` à jour
  (5 commits poussés). Dépôt privé `3` également à jour : toutes les petites
  modifications de cette session (corrections firmware, doc pilotage pompe,
  README, capture dashboard, ce fichier lui-même) ont été validées et
  poussées.
- **Logs série** : `moniteur_serie.py` enregistre désormais deux fichiers
  séparés (fonctionnement/défauts) dans `C:\rust\3\logs`, non commités.

## À faire, par ordre de proximité

**Chantiers matériels, rien codé, déjà câblés ou prévus** :
- Interrupteur logs série (GPIO27).
- Carte SD (broches confirmées CS=5/CLK=18/MISO=19/MOSI=23).
- Alarme coffret chaud (ventilation via AHT10 + vérif INA219).
- Capteur de courant TA12-100 pour la pompe (remplace le relais miroir).
- Alimentation batterie/solaire (idée, pas décidée).

**Discord / présentation** :
- Niveau 9/10 — en attente du niveau 10 pour publier le post de présentation
  déjà rédigé.
- Deux documents envisagés (doc technique + manuel utilisateur) — pas
  commencés, en partie déjà couverts par le nouveau README et la doc
  pilotage pompe.

**Idée non décidée, juste en mémoire** :
- Traduction anglaise de la doc à terme, si besoin de portée plus large —
  pas de date.
