//! Envoi vers Adafruit IO (cloud gratuit), en parallèle de ThingSpeak (voir
//! `thingspeak.rs` et la mémoire projet-cloud-thingspeak-alertes) : mêmes
//! mesures, tableaux de bord plus modernes à comparer. Feeds attendus côté
//! Adafruit IO : `temp-eau`, `temp-air`, `humidite`, `pompe`, `mode`,
//! `batterie`, `sortie-5v`.
//!
//! Contrairement à ThingSpeak (un seul appel HTTP peut mettre à jour plusieurs
//! champs à la fois), l'API Adafruit IO ne met à jour qu'**un seul feed par
//! requête** — une mesure combinée se traduit donc par plusieurs requêtes HTTP
//! successives. Comme pour ThingSpeak, tout ça tourne dans un **fil dédié**,
//! jamais dans la boucle principale surveillée par le Task Watchdog (voir
//! `projet-piege-appels-reseau-bloquants` : un appel réseau bloquant dans la
//! boucle principale a déjà provoqué de vrais redémarrages).

use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::Duration;

use embedded_svc::http::client::Client as ClientHttp;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use log::{info, warn};

const DELAI_PAR_OPERATION: Duration = Duration::from_secs(3);
const TAILLE_PILE: usize = 10240;
/// Plus profonde que celle de ThingSpeak (4) : une seule mise à jour combinée
/// peut se décomposer en jusqu'à 7 requêtes successives (une par feed).
const PROFONDEUR_FILE: usize = 32;

/// Un envoi vers Adafruit IO. Les champs `None` sont simplement omis (aucune
/// requête n'est faite pour ce feed).
#[derive(Clone, Copy, Default, Debug)]
pub struct Mesures {
    pub temp_eau: Option<f32>,
    pub temp_air: Option<f32>,
    pub humidite: Option<f32>,
    pub pompe: Option<bool>,
    pub mode: Option<u8>,
    pub batterie: Option<f32>,
    pub sortie_5v: Option<f32>,
}

impl Mesures {
    /// Liste des (nom de feed, valeur texte) à envoyer, une entrée par mesure présente.
    fn envois(&self) -> Vec<(&'static str, String)> {
        let mut envois = Vec::new();
        if let Some(v) = self.temp_eau {
            envois.push(("temp-eau", format!("{v:.2}")));
        }
        if let Some(v) = self.temp_air {
            envois.push(("temp-air", format!("{v:.2}")));
        }
        if let Some(v) = self.humidite {
            envois.push(("humidite", format!("{v:.2}")));
        }
        if let Some(v) = self.pompe {
            envois.push(("pompe", i32::from(v).to_string()));
        }
        if let Some(v) = self.mode {
            envois.push(("mode", v.to_string()));
        }
        if let Some(v) = self.batterie {
            envois.push(("batterie", format!("{v:.2}")));
        }
        if let Some(v) = self.sortie_5v {
            envois.push(("sortie-5v", format!("{v:.2}")));
        }
        envois
    }
}

/// Poignée vers le fil d'envoi. `envoyer()` ne bloque jamais.
pub struct Expediteur {
    file: SyncSender<Mesures>,
}

impl Expediteur {
    /// Démarre le fil d'envoi dédié. À appeler une seule fois, au démarrage.
    pub fn demarrer(username: &'static str, cle: &'static str) -> anyhow::Result<Self> {
        let (file, reception) = mpsc::sync_channel::<Mesures>(PROFONDEUR_FILE);
        std::thread::Builder::new()
            .name("adafruit_io".into())
            .stack_size(TAILLE_PILE)
            .spawn(move || boucle_envoi(username, cle, reception))?;
        Ok(Expediteur { file })
    }

    /// Dépose des mesures dans la file d'envoi et rend la main immédiatement.
    /// Comportement identique à `thingspeak::Expediteur::envoyer` : file pleine
    /// ou fil arrêté ne sont jamais fatals, juste journalisés.
    pub fn envoyer(&self, mesures: Mesures) {
        match self.file.try_send(mesures) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                warn!("Adafruit IO : file d'envoi pleine, mesures abandonnées (réseau lent ?)")
            }
            Err(TrySendError::Disconnected(_)) => {
                warn!("Adafruit IO : fil d'envoi arrêté, mesures abandonnées")
            }
        }
    }
}

/// Boucle du fil dédié : attend des mesures, envoie chaque feed présent l'un
/// après l'autre. Non surveillé par le Task Watchdog, peut bloquer sur le
/// réseau sans conséquence pour le reste du programme.
fn boucle_envoi(username: &str, cle: &str, reception: Receiver<Mesures>) {
    while let Ok(mesures) = reception.recv() {
        for (feed, valeur) in mesures.envois() {
            if let Err(e) = envoyer_bloquant(username, cle, feed, &valeur) {
                warn!("Adafruit IO : échec de l'envoi de '{feed}' (ignoré) : {:?}", e);
            }
        }
    }
    warn!("Adafruit IO : fil d'envoi terminé");
}

fn envoyer_bloquant(username: &str, cle: &str, feed: &str, valeur: &str) -> anyhow::Result<()> {
    let url = format!("https://io.adafruit.com/api/v2/{username}/feeds/{feed}/data");
    let corps = format!(r#"{{"value":"{valeur}"}}"#);
    let longueur = corps.len().to_string();

    let connection = EspHttpConnection::new(&HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        timeout: Some(DELAI_PAR_OPERATION),
        ..Default::default()
    })?;
    let mut client = ClientHttp::wrap(connection);
    let headers = [
        ("X-AIO-Key", cle),
        ("Content-Type", "application/json"),
        ("Content-Length", longueur.as_str()),
    ];
    let mut request = client.post(&url, &headers)?;
    request.write(corps.as_bytes())?;
    request.flush()?;
    let response = request.submit()?;
    let statut = response.status();

    if (200..300).contains(&statut) {
        info!("Adafruit IO : '{feed}' envoyé (HTTP {statut})");
    } else {
        warn!("Adafruit IO : réponse inattendue pour '{feed}' (HTTP {statut})");
    }

    Ok(())
}
