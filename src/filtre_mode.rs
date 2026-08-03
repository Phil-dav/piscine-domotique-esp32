use std::time::{Duration, Instant};

use crate::etat_partage::Mode;

/// Délai de confirmation avant de valider un changement de mode physique. Volontairement
/// plus long que l'anti-rebond électrique (200 ms, voir `securite.rs`) : ici on filtre un
/// geste mécanique humain, pas un rebond de contact électrique — l'interrupteur 3 positions
/// n'a pas de liaison directe AUTO ↔ MANU, donc passer de l'un à l'autre traverse
/// nécessairement la position OFF pendant la manœuvre.
const DELAI_CONFIRMATION: Duration = Duration::from_millis(700);

/// Filtre le mode physique lu sur l'interrupteur 3 positions (AUTO/MANU/OFF) : un
/// changement n'est validé que s'il reste stable pendant `DELAI_CONFIRMATION`, pour ignorer
/// le passage transitoire par OFF lors d'une manœuvre AUTO ↔ MANU. Sans ce filtre, ce
/// passage se traduisait par un aller-retour parasite sur la timeline du dashboard, et par
/// une demande d'arrêt pompe réelle (bien que très brève) à chaque manœuvre.
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

        if self.candidat != self.stable && self.depuis.elapsed() >= DELAI_CONFIRMATION {
            self.stable = self.candidat.clone();
        }

        self.stable.clone()
    }
}
