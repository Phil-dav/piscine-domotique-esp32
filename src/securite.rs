use std::time::{Duration, Instant};

/// Délai de confirmation avant verrouillage : élimine les glitches I2C et le bruit
/// électrique (même durée que le projet C++ de référence).
const ANTI_REBOND: Duration = Duration::from_millis(200);

/// Suivi du défaut moteur (contact NF sur P7) avec anti-rebond et verrou de réarmement.
///
/// Contact fermé (fonctionnement normal) → PCF lit LOW.
/// Contact ouvert (disjoncteur déclenché ou fil coupé) → PCF lit HIGH.
/// Une fois confirmé, le défaut reste verrouillé même si le contact se referme,
/// jusqu'à un réarmement manuel explicite (page web).
pub struct SecuriteMoteur {
    verrouille: bool,
    confirme: bool,
    depuis: Option<Instant>,
}

impl SecuriteMoteur {
    pub fn nouveau() -> Self {
        SecuriteMoteur {
            verrouille: false,
            confirme: false,
            depuis: None,
        }
    }

    /// À appeler à chaque itération avec la lecture brute de P7 (`true` = contact ouvert, défaut).
    pub fn mettre_a_jour(&mut self, defaut_brut: bool) {
        if defaut_brut {
            let depuis = *self.depuis.get_or_insert_with(Instant::now);
            if depuis.elapsed() >= ANTI_REBOND {
                self.confirme = true;
                self.verrouille = true;
            }
        } else {
            self.depuis = None;
            self.confirme = false;
        }
    }

    /// Défaut actif : confirmé maintenant, ou verrouillé en attente de réarmement.
    pub fn actif(&self) -> bool {
        self.confirme || self.verrouille
    }

    pub fn verrouille(&self) -> bool {
        self.verrouille
    }

    /// Réarmement manuel (déclenché depuis la page web).
    pub fn rearmer(&mut self) {
        self.verrouille = false;
        self.confirme = false;
        self.depuis = None;
    }
}
