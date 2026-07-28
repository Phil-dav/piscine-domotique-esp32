use crate::stockage::Stockage;

const CLE_DEBUT: &str = "filt_debut_min";
const CLE_FIN: &str = "filt_fin_min";
const DEBUT_DEFAUT_MIN: u32 = 8 * 60;
const FIN_DEFAUT_MIN: u32 = 20 * 60;

/// Calcul des heures de filtration cibles selon la température de l'eau, et de la
/// plage horaire à respecter. Port du `WaterTempManager` C++ : formule T/2 (+1h si
/// eau ≥ 24°C), avec 3 régimes à hystérésis pour éviter les oscillations aux seuils.
pub struct FiltrationAuto {
    mode_hiver: bool,    // < 9.5°C entrée / > 10.5°C sortie — 2h fixes, plage 10h-16h
    mode_canicule: bool, // > 28.5°C entrée / < 27.5°C sortie — continue 24h/24
    mode_antigel: bool,  // < 4°C entrée / > 5°C sortie — continue 24h/24
    debut_configure_min: u32, // minutes depuis minuit, mode standard uniquement
    fin_configuree_min: u32,
}

impl FiltrationAuto {
    pub fn nouveau(stockage: &Stockage) -> Self {
        FiltrationAuto {
            mode_hiver: true, // sécurité par défaut au démarrage, comme le projet C++
            mode_canicule: false,
            mode_antigel: false,
            debut_configure_min: stockage.lire_u32(CLE_DEBUT, DEBUT_DEFAUT_MIN),
            fin_configuree_min: stockage.lire_u32(CLE_FIN, FIN_DEFAUT_MIN),
        }
    }

    /// Met à jour les régimes (hystérésis) et renvoie les heures de filtration cibles du jour.
    pub fn heures_cibles(&mut self, temperature_eau: f32) -> f32 {
        // 1. Anti-gel
        if !self.mode_antigel && temperature_eau < 4.0 {
            self.mode_antigel = true;
        }
        if self.mode_antigel && temperature_eau > 5.0 {
            self.mode_antigel = false;
        }
        if self.mode_antigel {
            return 24.0;
        }

        // 2. Canicule
        if !self.mode_canicule && temperature_eau > 28.5 {
            self.mode_canicule = true;
        }
        if self.mode_canicule && temperature_eau < 27.5 {
            self.mode_canicule = false;
        }
        if self.mode_canicule {
            return 24.0;
        }

        // 3. Hiver
        if self.mode_hiver && temperature_eau > 10.5 {
            self.mode_hiver = false;
        }
        if !self.mode_hiver && temperature_eau < 9.5 {
            self.mode_hiver = true;
        }
        if self.mode_hiver {
            return 2.0;
        }

        // 4/5. Température normale (10.5-28.5°C) : T/2, +1h bonus si eau chaude (>= 24°C)
        let brut = if temperature_eau >= 24.0 {
            temperature_eau / 2.0 + 1.0
        } else {
            temperature_eau / 2.0
        };
        (brut * 2.0).round() / 2.0 // arrondi à la demi-heure la plus proche
    }

    pub fn anti_gel_actif(&self) -> bool {
        self.mode_antigel
    }

    pub fn canicule_actif(&self) -> bool {
        self.mode_canicule
    }

    /// Plage horaire effective (début, fin en heures décimales), selon le régime actif.
    pub fn plage_horaire(&self) -> (f32, f32) {
        if self.mode_antigel || self.mode_canicule {
            (0.0, 24.0)
        } else if self.mode_hiver {
            (10.0, 16.0)
        } else {
            self.plage_configuree()
        }
    }

    /// Plage horaire configurée pour le mode standard (affichage/édition web),
    /// indépendante du régime actuellement actif.
    pub fn plage_configuree(&self) -> (f32, f32) {
        (
            self.debut_configure_min as f32 / 60.0,
            self.fin_configuree_min as f32 / 60.0,
        )
    }

    /// Modifie la plage horaire standard et la persiste. Renvoie `false` si invalide
    /// (bornes 0h-24h, ou début >= fin) — rien n'est alors modifié.
    pub fn definir_plage(&mut self, stockage: &mut Stockage, debut_h: f32, fin_h: f32) -> bool {
        if !(0.0..=23.5).contains(&debut_h) || !(0.5..=24.0).contains(&fin_h) || debut_h >= fin_h {
            return false;
        }
        self.debut_configure_min = (debut_h * 60.0).round() as u32;
        self.fin_configuree_min = (fin_h * 60.0).round() as u32;
        stockage.ecrire_u32(CLE_DEBUT, self.debut_configure_min);
        stockage.ecrire_u32(CLE_FIN, self.fin_configuree_min);
        true
    }
}
