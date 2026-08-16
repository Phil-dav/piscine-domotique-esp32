use esp_idf_svc::hal::delay::Delay;
use esp_idf_svc::hal::interrupt;
use log::{info, warn};
use one_wire_bus::{Address, OneWire};
use std::time::{Duration, Instant};

/// Durée de conversion du DS18B20 en résolution 12 bits — la même valeur que
/// `Resolution::Bits12.delay_for_measurement_time()`, mais appliquée sans bloquer :
/// on revient lire le résultat au tick suivant plutôt que d'attendre sur place, pour ne
/// pas geler le reste de la boucle (bouton, etc.) pendant ~750 ms.
const DUREE_CONVERSION: Duration = Duration::from_millis(750);
/// Pas besoin de redemander une conversion en continu.
///
/// Porté de 2 s à 10 s le 16/08/2026. Chaque lecture ouvre deux fenêtres `interrupt::free`
/// (déclenchement puis lecture) : espacer divise par 5 le nombre de ces coupures, et donc
/// la gêne infligée au Wi-Fi. Sans perte d'information — une eau de piscine varie de
/// quelques centièmes de degré par heure, 10 s reste six fois par minute.
const INTERVALLE_LECTURE: Duration = Duration::from_secs(10);
/// Échecs de lecture consécutifs avant d'oublier l'adresse mémorisée et de forcer une
/// redécouverte complète du bus (précédée d'une impulsion de reset) : c'est la relance
/// automatique de la sonde, sans intervention manuelle.
const ECHECS_AVANT_REDECOUVERTE: u32 = 5;
/// Au-delà de ce délai sans aucune lecture valide, la sonde est signalée comme muette
/// (alerte journalisée + affichage au dashboard). Depuis le passage de l'intervalle de
/// lecture à 10 s (16/08/2026), ce délai représente une trentaine de tentatives échouées —
/// contre ~150 auparavant. C'est toujours un vrai décrochage et non du bruit, et le seuil
/// reste volontairement à 5 minutes : c'est une durée d'absence de mesure qui compte ici,
/// pas un nombre d'essais.
const DELAI_SONDE_MUETTE: Duration = Duration::from_secs(300);

enum Etat {
    Repos { depuis: Instant },
    Conversion { depuis: Instant },
}

/// Lecture non bloquante de la température eau (DS18B20, bus 1-Wire). Conserve la
/// dernière valeur valide en cas d'échec ponctuel (CRC, capteur non détecté...).
///
/// L'adresse du capteur est **mémorisée** après la première découverte : l'énumération
/// du bus (SEARCH ROM) est l'étape la plus fragile du protocole 1-Wire (parcours d'arbre
/// binaire bit à bit, sensible au bruit électrique), et la refaire à chaque lecture
/// provoquait des décrochages `UnexpectedResponse` alors que le capteur répondait très
/// bien. Une fois l'adresse connue, les lectures la ciblent directement (MATCH ROM).
pub struct SondeEau {
    etat: Etat,
    derniere_temperature: Option<f32>,
    derniere_lecture_ok: Option<Instant>,
    depuis_creation: Instant,
    adresse: Option<Address>,
    echecs_consecutifs: u32,
}

impl SondeEau {
    pub fn nouveau() -> Self {
        let maintenant = Instant::now();
        SondeEau {
            etat: Etat::Repos {
                depuis: maintenant - INTERVALLE_LECTURE, // déclenche une lecture dès le premier tick
            },
            derniere_temperature: None,
            derniere_lecture_ok: None,
            depuis_creation: maintenant,
            adresse: None,
            echecs_consecutifs: 0,
        }
    }

    /// À appeler à chaque tick de la boucle principale. Ne bloque jamais plus longtemps
    /// qu'une transaction 1-Wire ponctuelle (quelques ms) ; la conversion (~750 ms) est
    /// étalée sur plusieurs tours de boucle au lieu d'être attendue sur place.
    pub fn mettre_a_jour<P, E>(
        &mut self,
        one_wire: &mut OneWire<P>,
        delay: &mut Delay,
    ) -> Option<f32>
    where
        P: embedded_hal::digital::v2::InputPin<Error = E>
            + embedded_hal::digital::v2::OutputPin<Error = E>,
        E: core::fmt::Debug,
    {
        match self.etat {
            Etat::Repos { depuis } => {
                if depuis.elapsed() >= INTERVALLE_LECTURE {
                    // SKIP ROM : ne nécessite pas l'adresse, et envoie déjà une impulsion
                    // de reset sur le bus. Interruptions coupées sur ce cœur pendant la
                    // transaction (quelques ms) : le minutage 1-Wire, à la microseconde,
                    // ne supporte aucune préemption (voir main.rs pour le contexte : le
                    // Wi-Fi tourne sur ce même cœur et perturbait ces lectures).
                    let resultat = interrupt::free(|| {
                        ::ds18b20::start_simultaneous_temp_measurement(one_wire, delay)
                    });
                    match resultat {
                        Ok(()) => {
                            self.etat = Etat::Conversion {
                                depuis: Instant::now(),
                            }
                        }
                        Err(e) => {
                            warn!("DS18B20 : erreur déclenchement mesure : {:?}", e);
                            self.signaler_echec(one_wire, delay);
                            self.etat = Etat::Repos {
                                depuis: Instant::now(),
                            };
                        }
                    }
                }
            }
            Etat::Conversion { depuis } => {
                if depuis.elapsed() >= DUREE_CONVERSION {
                    match self.lire(one_wire, delay) {
                        Some(t) => {
                            self.derniere_temperature = Some(t);
                            self.derniere_lecture_ok = Some(Instant::now());
                            self.echecs_consecutifs = 0;
                        }
                        None => self.signaler_echec(one_wire, delay),
                    }
                    self.etat = Etat::Repos {
                        depuis: Instant::now(),
                    };
                }
            }
        }

        self.derniere_temperature
    }

    /// Lit la température : découvre l'adresse du capteur si elle n'est pas encore
    /// connue, puis interroge directement cette adresse.
    fn lire<P, E>(&mut self, one_wire: &mut OneWire<P>, delay: &mut Delay) -> Option<f32>
    where
        P: embedded_hal::digital::v2::InputPin<Error = E>
            + embedded_hal::digital::v2::OutputPin<Error = E>,
        E: core::fmt::Debug,
    {
        let adresse = match self.adresse {
            Some(a) => a,
            None => {
                // Étape la plus sensible (parcours d'arbre binaire bit à bit) :
                // interruptions coupées sur ce cœur pour toute sa durée.
                let trouvee = interrupt::free(|| chercher_capteur(one_wire, delay))?;
                info!("DS18B20 : capteur trouvé, adresse mémorisée");
                self.adresse = Some(trouvee);
                trouvee
            }
        };

        let capteur = match ::ds18b20::Ds18b20::new::<E>(adresse) {
            Ok(c) => c,
            Err(e) => {
                warn!("DS18B20 : adresse invalide : {:?}", e);
                self.adresse = None; // adresse douteuse : on la redécouvrira
                return None;
            }
        };

        let resultat = interrupt::free(|| capteur.read_data(one_wire, delay));
        match resultat {
            Ok(data) => Some(data.temperature),
            Err(e) => {
                warn!("DS18B20 : erreur lecture donnée (ignorée) : {:?}", e);
                None
            }
        }
    }

    /// Compte un échec et, au-delà du seuil, force une relance complète : l'adresse
    /// mémorisée est oubliée et une impulsion de reset est envoyée sur le bus pour
    /// remettre les esclaves dans un état connu avant la redécouverte.
    fn signaler_echec<P, E>(&mut self, one_wire: &mut OneWire<P>, delay: &mut Delay)
    where
        P: embedded_hal::digital::v2::InputPin<Error = E>
            + embedded_hal::digital::v2::OutputPin<Error = E>,
        E: core::fmt::Debug,
    {
        self.echecs_consecutifs += 1;
        if self.echecs_consecutifs < ECHECS_AVANT_REDECOUVERTE {
            return;
        }

        self.echecs_consecutifs = 0;
        self.adresse = None;
        match interrupt::free(|| one_wire.reset(delay)) {
            Ok(true) => info!("DS18B20 : relance du bus 1-Wire, un capteur a répondu"),
            Ok(false) => warn!("DS18B20 : relance du bus 1-Wire, aucun capteur n'a répondu"),
            Err(e) => warn!("DS18B20 : échec de la relance du bus 1-Wire : {:?}", e),
        }
    }

    /// `true` si aucune lecture valide n'a abouti depuis `DELAI_SONDE_MUETTE` (le délai
    /// court depuis le démarrage tant qu'aucune lecture n'a jamais réussi).
    pub fn muette(&self) -> bool {
        let reference = self.derniere_lecture_ok.unwrap_or(self.depuis_creation);
        reference.elapsed() >= DELAI_SONDE_MUETTE
    }

    /// Secondes écoulées depuis la dernière lecture valide (`None` si jamais aucune).
    pub fn secondes_depuis_lecture(&self) -> Option<u32> {
        self.derniere_lecture_ok
            .map(|t| t.elapsed().as_secs() as u32)
    }
}

/// Énumère le bus et renvoie l'adresse du premier DS18B20 trouvé.
fn chercher_capteur<P, E>(one_wire: &mut OneWire<P>, delay: &mut Delay) -> Option<Address>
where
    P: embedded_hal::digital::v2::InputPin<Error = E>
        + embedded_hal::digital::v2::OutputPin<Error = E>,
    E: core::fmt::Debug,
{
    for resultat in one_wire.devices(false, delay) {
        match resultat {
            Ok(adresse) if adresse.family_code() == ::ds18b20::FAMILY_CODE => return Some(adresse),
            Ok(_) => {} // autre famille de composant sur le bus : ignoré
            Err(e) => {
                warn!("DS18B20 : erreur recherche 1-Wire : {:?}", e);
                return None;
            }
        }
    }
    None
}
