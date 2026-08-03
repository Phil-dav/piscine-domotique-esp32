use std::time::{Duration, Instant};

// Hystérésis analogique (échelle ADC 12 bits, 0-4095) — mêmes seuils que le projet C++.
const SEUIL_BAS: u16 = 1800; // En dessous : danger certain
const SEUIL_HAUT: u16 = 2200; // Au dessus : OK certain

// Temporisation avant de valider un changement d'état (anti-vagues).
const DELAI_ARRET: Duration = Duration::from_millis(3000); // confirmation avant coupure
const DELAI_OK: Duration = Duration::from_millis(5000); // confirmation avant réarmement

/// Lecture du niveau d'eau avec hystérésis + filtrage temporel, pour ignorer les
/// vaguelettes et le bruit du capteur sans réagir trop lentement à un vrai manque d'eau.
pub struct NiveauEau {
    filtre: bool,
    cible: bool,
    depuis: Instant,
    premiere_lecture: bool,
}

impl NiveauEau {
    pub fn nouveau() -> Self {
        NiveauEau {
            // Valeurs de départ sans importance : écrasées par la vraie mesure dès
            // le premier appel à `mettre_a_jour` (voir `premiere_lecture`), donc pas
            // de fenêtre au démarrage où un manque d'eau réel serait ignoré.
            filtre: true,
            cible: true,
            depuis: Instant::now(),
            premiere_lecture: true,
        }
    }

    /// Met à jour l'état à partir d'une lecture ADC brute (0-4095) ; renvoie `true` si le niveau est correct.
    pub fn mettre_a_jour(&mut self, valeur_brute: u16) -> bool {
        let mut brut = self.cible; // zone morte : on garde la décision précédente entre les deux seuils
        if valeur_brute < SEUIL_BAS {
            brut = false;
        } else if valeur_brute > SEUIL_HAUT {
            brut = true;
        }

        if self.premiere_lecture {
            // Première mesure réelle après le démarrage : on l'applique tout de
            // suite, sans délai de confirmation ni supposition optimiste — ni trop
            // prudent (bloquer sans raison), ni trop confiant (laisser tourner à
            // sec le temps que le filtre anti-vaguelettes se stabilise).
            self.cible = brut;
            self.filtre = brut;
            self.depuis = Instant::now();
            self.premiere_lecture = false;
            return self.filtre;
        }

        if brut != self.cible {
            self.cible = brut;
            self.depuis = Instant::now();
        }

        let delai_requis = if self.cible { DELAI_OK } else { DELAI_ARRET };
        if self.depuis.elapsed() >= delai_requis {
            self.filtre = self.cible;
        }

        self.filtre
    }
}
