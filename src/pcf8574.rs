use core::cell::RefCell;
use esp_idf_svc::hal::i2c::I2cDriver;

use crate::config::{
    PCF8574_ADDR, PCF_BOUTON_OLED, PCF_DEFAUT_RELAIS, PCF_MODE_AUTO, PCF_MODE_MANU,
    PCF_RELAIS_DEFAUT_SYSTEME, PCF_RELAIS_POMPE,
};
use crate::etat_partage::Mode;

fn ecrire(i2c: &RefCell<I2cDriver>, valeur: u8) -> anyhow::Result<()> {
    i2c.borrow_mut()
        .write(PCF8574_ADDR, &[valeur], 100)
        .map_err(|e| anyhow::anyhow!("Erreur I2C (écriture PCF8574) : {:?}", e))
}

fn lire(i2c: &RefCell<I2cDriver>) -> anyhow::Result<u8> {
    let mut buffer = [0u8; 1];
    i2c.borrow_mut()
        .read(PCF8574_ADDR, &mut buffer, 100)
        .map_err(|e| anyhow::anyhow!("Erreur I2C (lecture PCF8574) : {:?}", e))?;
    Ok(buffer[0])
}

/// Mémorise l'état voulu des 4 sorties (P0-P3). Le PCF8574 n'a qu'un seul registre
/// pour les 8 broches : impossible de modifier une seule sortie sans renvoyer tout
/// l'octet, donc sans garder ce qu'on a déjà écrit on effacerait les autres sorties
/// à chaque appel (ex. éteindre le relais défaut en pilotant la pompe).
pub struct Sorties {
    octet: u8,
}

impl Sorties {
    fn appliquer(
        &mut self,
        i2c: &RefCell<I2cDriver>,
        broche: u8,
        actif: bool,
    ) -> anyhow::Result<()> {
        if actif {
            self.octet &= !(1 << broche); // LOW = actif
        } else {
            self.octet |= 1 << broche;
        }
        ecrire(i2c, self.octet | 0xF0) // P4-P7 toujours hauts (entrées)
    }
}

/// Prépare le PCF8574 : sorties (P0-P3) inactives, entrées (P4-P7) prêtes à être lues.
/// Voir `config.rs` pour le rôle de chaque broche (mapping du circuit physique).
pub fn init(i2c: &RefCell<I2cDriver>) -> anyhow::Result<Sorties> {
    ecrire(i2c, 0xFF)?;
    Ok(Sorties { octet: 0xFF })
}

/// Lit la position de l'interrupteur physique AUTO/MANU (P5/P6).
/// `Mode::Off` si aucune des deux positions n'est active (interrupteur à mi-course, ou lecture incohérente).
pub fn lire_mode(i2c: &RefCell<I2cDriver>) -> anyhow::Result<Mode> {
    let etat = lire(i2c)?;
    let manu_actif = (etat >> PCF_MODE_MANU) & 1 == 0; // LOW = actif
    let auto_actif = (etat >> PCF_MODE_AUTO) & 1 == 0;
    Ok(match (auto_actif, manu_actif) {
        (true, false) => Mode::Auto,
        (false, true) => Mode::Manuel,
        _ => Mode::Off,
    })
}

/// Lit l'entrée défaut moteur (P7). Contrairement aux autres entrées (P4-P6, actives
/// à LOW via les résistances de tirage R2-R5), P7 est câblée sur un contact NF du
/// disjoncteur moteur : contact fermé (normal) → LOW, contact ouvert (défaut) → HIGH.
pub fn lire_defaut_moteur(i2c: &RefCell<I2cDriver>) -> anyhow::Result<bool> {
    let etat = lire(i2c)?;
    Ok((etat >> PCF_DEFAUT_RELAIS) & 1 == 1)
}

/// Lit le bouton écran (P4). `true` si appuyé (actif à LOW, comme P5/P6).
pub fn lire_bouton_oled(i2c: &RefCell<I2cDriver>) -> anyhow::Result<bool> {
    let etat = lire(i2c)?;
    Ok((etat >> PCF_BOUTON_OLED) & 1 == 0)
}

/// Pilote le relais pompe (P0).
pub fn piloter_pompe(
    sorties: &mut Sorties,
    i2c: &RefCell<I2cDriver>,
    actif: bool,
) -> anyhow::Result<()> {
    sorties.appliquer(i2c, PCF_RELAIS_POMPE, actif)
}

/// Pilote le relais de signalisation défaut système (P3).
pub fn piloter_defaut_systeme(
    sorties: &mut Sorties,
    i2c: &RefCell<I2cDriver>,
    actif: bool,
) -> anyhow::Result<()> {
    sorties.appliquer(i2c, PCF_RELAIS_DEFAUT_SYSTEME, actif)
}
