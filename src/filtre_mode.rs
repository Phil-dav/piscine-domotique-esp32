use std::time::{Duration, Instant};

use crate::etat_partage::Mode;

/// Délai de confirmation avant de valider une sélection de mode (AUTO ou MANU).
/// Volontairement plus long que l'anti-rebond électrique (200 ms, voir `securite.rs`) :
/// ici on filtre un geste mécanique humain, pas un rebond de contact électrique.
const DELAI_CONFIRMATION: Duration = Duration::from_millis(700);

/// Délai de confirmation spécifique à la position ARRÊT, nettement plus long.
///
/// L'interrupteur 3 positions n'a pas de liaison directe AUTO ↔ MANU : sa position
/// centrale est l'ARRÊT (aucun contact fermé, voir `pcf8574::lire_mode`), donc passer
/// d'un mode à l'autre traverse **nécessairement** l'ARRÊT pendant la manœuvre.
///
/// 700 ms se sont révélés insuffisants : l'utilisateur a constaté le 15/08/2026 que
/// l'ARRÊT apparaissait quand même sur la timeline du dashboard lors d'un basculement
/// AUTO ↔ MANU — une manœuvre humaine sur un inverseur à cran dépasse facilement ce délai.
///
/// D'où deux délais distincts plutôt qu'un seul allongé : le passage parasite est
/// **toujours** un passage par ARRÊT, jamais par AUTO ou MANU. Allonger uniquement
/// celui-ci filtre le transitoire sans retarder la sélection d'un mode.
///
/// Contrepartie assumée : un arrêt volontaire demande de tenir la position 2 s avant
/// d'être pris en compte. Sans conséquence sur une pompe de filtration, qui met de
/// toute façon plusieurs secondes à s'arrêter réellement.
const DELAI_CONFIRMATION_ARRET: Duration = Duration::from_secs(2);

/// Filtre le mode physique lu sur l'interrupteur 3 positions (AUTO/MANU/OFF) : un
/// changement n'est validé que s'il reste stable pendant le délai correspondant, pour
/// ignorer le passage transitoire par OFF lors d'une manœuvre AUTO ↔ MANU. Sans ce filtre,
/// ce passage se traduisait par un aller-retour parasite sur la timeline du dashboard, et
/// par une demande d'arrêt pompe réelle (bien que très brève) à chaque manœuvre.
pub struct FiltreMode {
    stable: Mode,
    candidat: Mode,
    depuis: Instant,
    premiere_lecture: bool,
}

impl FiltreMode {
    pub fn nouveau() -> Self {
        FiltreMode {
            stable: Mode::Off,
            candidat: Mode::Off,
            depuis: Instant::now(),
            premiere_lecture: true,
        }
    }

    /// À appeler à chaque tour de boucle avec la lecture brute de l'interrupteur.
    /// Renvoie le mode confirmé, à utiliser partout ailleurs dans le programme.
    pub fn mettre_a_jour(&mut self, lu: Mode) -> Mode {
        if self.premiere_lecture {
            // Première lecture après démarrage : appliquée immédiatement, comme pour
            // le niveau d'eau — pas de raison d'attendre pour la toute première valeur.
            self.stable = lu.clone();
            self.candidat = lu;
            self.depuis = Instant::now();
            self.premiere_lecture = false;
            return self.stable.clone();
        }

        if lu != self.candidat {
            self.candidat = lu;
            self.depuis = Instant::now();
        }

        // Le délai dépend de la position visée : long vers l'ARRÊT (pour absorber la
        // traversée de la position centrale), court vers AUTO/MANU (pour rester réactif).
        let delai = if self.candidat == Mode::Off {
            DELAI_CONFIRMATION_ARRET
        } else {
            DELAI_CONFIRMATION
        };

        if self.candidat != self.stable && self.depuis.elapsed() >= delai {
            self.stable = self.candidat.clone();
        }

        self.stable.clone()
    }
}
