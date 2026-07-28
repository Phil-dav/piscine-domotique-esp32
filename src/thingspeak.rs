//! Envoi vers un canal ThingSpeak (cloud gratuit), pour consulter des graphiques
//! depuis n'importe où sans dépendre d'un PC allumé en continu : les mesures
//! (température eau/air, humidité) toutes les 5 minutes, et l'état de la pompe
//! (Field 4, 0/1) ainsi que le mode (Field 5, 0=OFF/1=MANU/2=AUTO) uniquement à
//! chaque changement, pour un vrai signal en créneaux plutôt qu'un échantillonnage
//! périodique (choisir le type de tracé "step" côté ThingSpeak pour un rendu net).
//!
//! **L'appel réseau tourne dans un fil d'exécution dédié**, pas dans la boucle
//! principale. Raison (incident du 27/07/2026, redémarrages `WATCHDOG_TACHE`) : le
//! `timeout` du client HTTP d'ESP-IDF n'est pas un délai global pour la requête,
//! mais un délai appliqué à *chaque* opération réseau élémentaire (`SO_RCVTIMEO` du
//! socket, voir `esp-tls/esp_tls.c`). Une poignée de main TLS demandant plusieurs
//! allers-retours, un Wi-Fi qui perd des paquets peut donc bloquer l'appel bien
//! au-delà du délai configuré — dépassant les 5 s du Task Watchdog et provoquant un
//! redémarrage. Aucun réglage de délai ne peut rendre cet appel sûr dans la boucle
//! principale : il faut l'en sortir.

use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::Duration;

use embedded_svc::http::client::Client as ClientHttp;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use log::{info, warn};

/// Délai par opération réseau (voir l'explication en tête de module : ce n'est PAS
/// une garantie sur la durée totale de la requête). Sert surtout à ne pas rester
/// indéfiniment sur un serveur muet.
const DELAI_PAR_OPERATION: Duration = Duration::from_secs(3);
/// Le fil réseau fait du TLS (mbedtls) : la pile par défaut des threads
/// (`CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT`, 4 Ko) serait trop juste.
const TAILLE_PILE: usize = 10240;
/// Quelques envois peuvent attendre si le réseau est lent, sans jamais bloquer la
/// boucle principale. Au-delà, les mesures les plus récentes sont abandonnées
/// (perdre un point de courbe est sans conséquence, bloquer la pompe ne l'est pas).
const PROFONDEUR_FILE: usize = 4;

/// Un envoi vers ThingSpeak. Les champs `None` sont omis de la requête : ThingSpeak
/// conserve alors la dernière valeur connue pour ces champs.
#[derive(Clone, Copy, Default, Debug)]
pub struct Mesures {
    pub temp_eau: Option<f32>,
    pub temp_air: Option<f32>,
    pub humidite: Option<f32>,
    pub pompe: Option<bool>,
    pub mode: Option<u8>,
}

impl Mesures {
    /// Paramètres d'URL correspondants (`&field1=...`), vide si rien à envoyer.
    fn parametres(&self) -> String {
        let mut champs = String::new();
        if let Some(v) = self.temp_eau {
            champs.push_str(&format!("&field1={v:.2}"));
        }
        if let Some(v) = self.temp_air {
            champs.push_str(&format!("&field2={v:.2}"));
        }
        if let Some(v) = self.humidite {
            champs.push_str(&format!("&field3={v:.2}"));
        }
        if let Some(v) = self.pompe {
            champs.push_str(&format!("&field4={}", i32::from(v)));
        }
        if let Some(v) = self.mode {
            champs.push_str(&format!("&field5={v}"));
        }
        champs
    }
}

/// Poignée vers le fil d'envoi. `envoyer()` ne bloque jamais.
pub struct Expediteur {
    file: SyncSender<Mesures>,
}

impl Expediteur {
    /// Démarre le fil d'envoi dédié. À appeler une seule fois, au démarrage.
    pub fn demarrer(api_key: &'static str) -> anyhow::Result<Self> {
        let (file, reception) = mpsc::sync_channel::<Mesures>(PROFONDEUR_FILE);
        std::thread::Builder::new()
            .name("thingspeak".into())
            .stack_size(TAILLE_PILE)
            .spawn(move || boucle_envoi(api_key, reception))?;
        Ok(Expediteur { file })
    }

    /// Dépose des mesures dans la file d'envoi et rend la main immédiatement.
    /// N'échoue jamais de façon fatale : si la file est pleine (réseau lent, envois
    /// précédents encore en cours), les mesures sont simplement abandonnées avec un
    /// avertissement — jamais au prix d'un blocage de la boucle principale.
    pub fn envoyer(&self, mesures: Mesures) {
        match self.file.try_send(mesures) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                warn!("ThingSpeak : file d'envoi pleine, mesures abandonnées (réseau lent ?)")
            }
            Err(TrySendError::Disconnected(_)) => {
                warn!("ThingSpeak : fil d'envoi arrêté, mesures abandonnées")
            }
        }
    }
}

/// Boucle du fil dédié : attend des mesures et les envoie, sans limite de durée.
/// Ce fil n'est pas surveillé par le Task Watchdog (seule la tâche principale l'est,
/// voir `main.rs`), il peut donc bloquer sur le réseau sans conséquence.
fn boucle_envoi(api_key: &str, reception: Receiver<Mesures>) {
    while let Ok(mesures) = reception.recv() {
        if let Err(e) = envoyer_bloquant(api_key, &mesures) {
            warn!("ThingSpeak : échec de l'envoi (ignoré) : {:?}", e);
        }
    }
    warn!("ThingSpeak : fil d'envoi terminé");
}

fn envoyer_bloquant(api_key: &str, mesures: &Mesures) -> anyhow::Result<()> {
    let champs = mesures.parametres();
    if champs.is_empty() {
        // Aucune mesure disponible (capteurs pas encore lus) : rien à envoyer.
        return Ok(());
    }

    let url = format!("https://api.thingspeak.com/update?api_key={api_key}{champs}");

    let connection = EspHttpConnection::new(&HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        timeout: Some(DELAI_PAR_OPERATION),
        ..Default::default()
    })?;
    let mut client = ClientHttp::wrap(connection);
    let request = client.get(&url)?;
    let response = request.submit()?;
    let statut = response.status();

    if (200..300).contains(&statut) {
        info!("ThingSpeak : mesures envoyées (HTTP {statut})");
    } else {
        warn!("ThingSpeak : réponse inattendue (HTTP {statut})");
    }

    Ok(())
}
