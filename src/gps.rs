use std::time::{Duration, Instant};

use chrono::NaiveDate;
use esp_idf_svc::hal::uart::UartDriver;
use nmea0183::{ParseResult, Parser};

/// Durée au-delà de laquelle une donnée GPS est considérée périmée
/// (même seuil que le projet C++ de référence : `age() < 5000` ms).
const FRAICHEUR_MAX: Duration = Duration::from_millis(5000);
/// Nombre minimum de satellites pour considérer le fix fiable.
const SATELLITES_MIN: u8 = 4;

/// Lecture et décodage NMEA du module GPS, avec suivi de fraîcheur des données.
pub struct Gps {
    uart: UartDriver<'static>,
    parser: Parser,
    dernier_temps_utc: Option<(NaiveDate, u32, Instant)>, // (date, secondes depuis minuit, horodatage local de réception)
    satellites: u8,
    dernier_satellites: Option<Instant>,
    derniere_position: Option<(f64, f64, Instant)>, // (latitude, longitude, horodatage local de réception)
}

impl Gps {
    pub fn new(uart: UartDriver<'static>) -> Self {
        Gps {
            uart,
            parser: Parser::new(),
            dernier_temps_utc: None,
            satellites: 0,
            dernier_satellites: None,
            derniere_position: None,
        }
    }

    /// Lit les octets disponibles sur l'UART (non bloquant) et met à jour l'état GPS.
    pub fn mettre_a_jour(&mut self) -> anyhow::Result<()> {
        let mut buffer = [0u8; 64];
        loop {
            match self
                .uart
                .read(&mut buffer, esp_idf_svc::hal::delay::NON_BLOCK)
            {
                Ok(lus) => {
                    for &octet in &buffer[..lus] {
                        if let Some(Ok(resultat)) = self.parser.parse_from_byte(octet) {
                            self.traiter_trame(resultat);
                        }
                    }
                }
                // Aucune donnée disponible pour l'instant : comportement normal
                // d'une lecture non bloquante, pas une erreur (voir doc UartRxDriver::read).
                Err(e) if e.code() == esp_idf_svc::sys::ESP_ERR_TIMEOUT => break,
                Err(e) => return Err(anyhow::anyhow!("Erreur lecture UART GPS : {:?}", e)),
            }
        }
        Ok(())
    }

    fn traiter_trame(&mut self, resultat: ParseResult) {
        match resultat {
            ParseResult::RMC(Some(rmc)) => {
                if let Some(date) = NaiveDate::from_ymd_opt(
                    rmc.datetime.date.year as i32,
                    rmc.datetime.date.month as u32,
                    rmc.datetime.date.day as u32,
                ) {
                    let secondes_depuis_minuit = rmc.datetime.time.hours as u32 * 3600
                        + rmc.datetime.time.minutes as u32 * 60
                        + rmc.datetime.time.seconds as u32;
                    self.dernier_temps_utc = Some((date, secondes_depuis_minuit, Instant::now()));
                }
            }
            ParseResult::GGA(Some(gga)) => {
                self.satellites = gga.sat_in_use;
                self.dernier_satellites = Some(Instant::now());
                self.derniere_position = Some((
                    gga.latitude.as_f64(),
                    gga.longitude.as_f64(),
                    Instant::now(),
                ));
            }
            _ => {}
        }
    }

    /// `true` si le fix GPS est frais (< 5 s) et suffisamment de satellites (≥ 4),
    /// à l'image de la condition utilisée côté C++.
    pub fn valide(&self) -> bool {
        let temps_frais = self
            .dernier_temps_utc
            .is_some_and(|(_, _, t)| t.elapsed() < FRAICHEUR_MAX);
        let satellites_frais = self
            .dernier_satellites
            .is_some_and(|t| t.elapsed() < FRAICHEUR_MAX);
        temps_frais && satellites_frais && self.satellites >= SATELLITES_MIN
    }

    /// Nombre de satellites utilisés, seulement si la donnée est fraîche (sinon 0).
    pub fn satellites(&self) -> u8 {
        match self.dernier_satellites {
            Some(t) if t.elapsed() < FRAICHEUR_MAX => self.satellites,
            _ => 0,
        }
    }

    /// Position (latitude, longitude) en degrés décimaux, seulement si la donnée est
    /// fraîche (< 5 s) — négatif pour Sud/Ouest, positif pour Nord/Est.
    pub fn position(&self) -> Option<(f64, f64)> {
        let (lat, lon, horodatage) = self.derniere_position?;
        if horodatage.elapsed() >= FRAICHEUR_MAX {
            return None;
        }
        Some((lat, lon))
    }

    /// Date/heure UTC courante telle que reçue du GPS (sans correction de fuseau).
    pub fn temps_utc(&self) -> Option<chrono::NaiveDateTime> {
        let (date, secondes, horodatage) = self.dernier_temps_utc?;
        if horodatage.elapsed() >= FRAICHEUR_MAX {
            return None;
        }
        date.and_hms_opt(secondes / 3600, (secondes / 60) % 60, secondes % 60)
    }
}
