//! Lecture des deux ponts diviseurs de tension du système batterie/solaire
//! (voir schéma "Lecture Tension Batterie" et mémoire
//! `projet-alimentation-batterie-solaire`) :
//! - **MTB 12** (GPIO32) : branché directement sur les bornes de la batterie
//!   12V (R22 = 56k côté +, R23 = 10k côté masse). Suit sa charge réelle au
//!   fil du temps.
//! - **MTB 5** (GPIO35) : branché sur la sortie 5V régulée du contrôleur
//!   solaire (R20 = 10k côté +, R21 = 15k côté masse). Reste quasiment plate
//!   tant que le régulateur tient sa consigne : sert d'alerte de dernière
//!   minute juste avant que la batterie ne soit trop faible pour le régulateur,
//!   pas d'un suivi progressif de charge (voir la mémoire pour l'explication).

use core::borrow::Borrow;

use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::adc::AdcChannel;
use log::warn;

/// Ratio du pont MTB 12 (batterie) : R23 / (R22 + R23).
pub const RATIO_MTB_12: f32 = 10.0 / (56.0 + 10.0);
/// Ratio du pont MTB 5 (sortie régulée 5V) : R21 / (R20 + R21).
pub const RATIO_MTB_5: f32 = 15.0 / (10.0 + 15.0);

/// Lit un pont diviseur et renvoie la tension d'origine (avant division), en
/// volts. `None` si la lecture ADC échoue (jamais fatal, comme les autres
/// capteurs du projet).
pub fn lire_volts<'d, C, M>(chan: &mut AdcChannelDriver<'d, C, M>, ratio: f32) -> Option<f32>
where
    C: AdcChannel,
    M: Borrow<AdcDriver<'d, C::AdcUnit>>,
{
    match chan.read() {
        Ok(mv) => Some(mv as f32 / 1000.0 / ratio),
        Err(e) => {
            warn!("Batterie : lecture ADC échouée : {:?}", e);
            None
        }
    }
}
