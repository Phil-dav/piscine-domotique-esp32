use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use log::warn;

const ESPACE_NOM: &str = "reglages";

/// Petite enveloppe autour de la NVS pour les réglages persistés (plage horaire,
/// durée du boost...). Une écriture échouée est juste signalée (`warn!`) : la valeur
/// reste appliquée en mémoire pour la session en cours, seule la persistance est perdue.
pub struct Stockage {
    nvs: EspNvs<NvsDefault>,
}

impl Stockage {
    pub fn ouvrir(partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(partition, ESPACE_NOM, true)
            .map_err(|e| anyhow::anyhow!("Erreur ouverture NVS ({}) : {:?}", ESPACE_NOM, e))?;
        Ok(Stockage { nvs })
    }

    pub fn lire_u32(&self, cle: &str, defaut: u32) -> u32 {
        self.nvs.get_u32(cle).ok().flatten().unwrap_or(defaut)
    }

    pub fn ecrire_u32(&mut self, cle: &str, valeur: u32) {
        if let Err(e) = self.nvs.set_u32(cle, valeur) {
            warn!("NVS : échec écriture '{}' : {:?}", cle, e);
        }
    }
}
