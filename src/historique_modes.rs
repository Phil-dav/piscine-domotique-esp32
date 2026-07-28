//! Historique coloré des modes de fonctionnement de la journée, pour la timeline
//! du dashboard (portage du `ModeHistory` du projet C++ de référence).
//!
//! Types de segment (doivent correspondre à `MODE_SEG_COLORS` dans `script.js`) :
//! 0 = OFF (gris), 1 = AUTO (bleu), 2 = MANU ON (vert), 3 = MANU OFF (orange),
//! 4 = BOOST ON (violet), 5 = BOOST OFF (rouge).

const MAX_SEGMENTS: usize = 64;

#[derive(Clone, Debug)]
pub struct Segment {
    pub debut: f32,
    pub fin: f32, // -1.0 = segment en cours (pas encore fermé)
    pub type_segment: u8,
}

pub struct HistoriqueModes {
    segments: Vec<Segment>,
}

impl HistoriqueModes {
    pub fn nouveau() -> Self {
        HistoriqueModes {
            segments: Vec::with_capacity(MAX_SEGMENTS),
        }
    }

    /// Reconstruit l'historique à partir de segments rechargés depuis la NVS
    /// (persistance après redémarrage, voir `journal::charger_timeline`).
    pub fn depuis_segments(segments: Vec<Segment>) -> Self {
        HistoriqueModes { segments }
    }

    /// Ferme le segment en cours (s'il y en a un) et en ouvre un nouveau.
    pub fn demarrer(&mut self, type_segment: u8, heure_decimale: f32) {
        if let Some(dernier) = self.segments.last_mut() {
            if dernier.fin < 0.0 {
                dernier.fin = heure_decimale;
            }
        }
        if self.segments.len() < MAX_SEGMENTS {
            self.segments.push(Segment {
                debut: heure_decimale,
                fin: -1.0,
                type_segment,
            });
        }
    }

    /// Remet à zéro l'historique (changement de jour).
    pub fn reinitialiser(&mut self) {
        self.segments.clear();
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }
}
