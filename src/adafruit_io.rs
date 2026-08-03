//! Envoi vers Adafruit IO (cloud gratuit) — seul service cloud du projet depuis
//! le retrait de ThingSpeak le 29/07/2026 (graphiques jugés moins bons par
//! l'utilisateur). Feeds attendus : `temp-eau`, `temp-air`, `humidite`,
//! `pompe`, `mode`, `batterie`, `sortie-5v`.
//!
//! L'API Adafruit IO de base ne met à jour qu'**un seul feed par requête** —
//! une mesure combinée se traduisait donc par jusqu'à 7 requêtes HTTP
//! successives. Cette charge réseau aggravait les erreurs CRC de la sonde
//! DS18B20 (contention avec le protocole 1-Wire, très sensible au minutage —
//! voir la mémoire projet-sonde-eau-decrochages, confirmé par test le
//! 28/07/2026). Correctif du 29/07/2026 : les 7 feeds sont rattachés à un
//! "Group" Adafruit IO (`config::ADAFRUIT_IO_GROUP_KEY`) et partent maintenant
//! tous ensemble en **une seule requête groupée** par cycle, via l'API "Group
//! Data" (`POST /groups/{groupe}/data`).
//!
//! Tout ça tourne dans un **fil dédié**, jamais dans la boucle principale
//! surveillée par le Task Watchdog (voir
//! `projet-piege-appels-reseau-bloquants` : un appel réseau bloquant dans la
//! boucle principale a déjà provoqué de vrais redémarrages).

use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::time::Duration;

use embedded_svc::http::client::Client as ClientHttp;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use log::{info, warn};

const DELAI_PAR_OPERATION: Duration = Duration::from_secs(3);
const TAILLE_PILE: usize = 10240;
const PROFONDEUR_FILE: usize = 32;
const TENTATIVES_MAX: u32 = 3;
const DELAI_ENTRE_TENTATIVES: Duration = Duration::from_secs(2);

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
    pub fn demarrer(
        username: &'static str,
        cle: &'static str,
        groupe: &'static str,
    ) -> anyhow::Result<Self> {
        let (file, reception) = mpsc::sync_channel::<Mesures>(PROFONDEUR_FILE);
        std::thread::Builder::new()
            .name("adafruit_io".into())
            .stack_size(TAILLE_PILE)
            .spawn(move || boucle_envoi(username, cle, groupe, reception))?;
        Ok(Expediteur { file })
    }

    /// Dépose des mesures dans la file d'envoi et rend la main immédiatement.
    /// File pleine ou fil arrêté ne sont jamais fatals, juste journalisés.
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

/// Boucle du fil dédié : envoie toutes les mesures présentes en une seule
/// requête groupée. Non surveillé par le Task Watchdog, peut bloquer sur le
/// réseau sans conséquence pour le reste du programme.
fn boucle_envoi(username: &str, cle: &str, groupe: &str, reception: Receiver<Mesures>) {
    while let Ok(mesures) = reception.recv() {
        let envois = mesures.envois();
        if !envois.is_empty() {
            // Une erreur réseau transitoire (Wi-Fi en pleine renégociation, etc.) ne doit
            // pas faire perdre tout le paquet : quelques essais rapprochés avant
            // d'abandonner. Sans conséquence sur la boucle principale ni le Task
            // Watchdog puisqu'on est déjà dans le fil dédié.
            let mut derniere_erreur = None;
            let mut reussi = false;
            for tentative in 1..=TENTATIVES_MAX {
                match envoyer_groupe_bloquant(username, cle, groupe, &envois) {
                    Ok(()) => {
                        reussi = true;
                        break;
                    }
                    Err(e) => {
                        derniere_erreur = Some(e);
                        if tentative < TENTATIVES_MAX {
                            std::thread::sleep(DELAI_ENTRE_TENTATIVES);
                        }
                    }
                }
            }
            if !reussi {
                warn!(
                    "Adafruit IO : échec de l'envoi groupé après {TENTATIVES_MAX} tentative(s) (ignoré) : {:?}",
                    derniere_erreur
                );
            }
        }
    }
    warn!("Adafruit IO : fil d'envoi terminé");
}

/// Envoie plusieurs feeds en une seule requête HTTP via l'API "Group Data"
/// d'Adafruit IO (`POST /groups/{groupe}/data`, corps `{"feeds":[{"key":...,
/// "value":...}, ...]}`) — voir le commentaire de module pour le contexte.
fn envoyer_groupe_bloquant(
    username: &str,
    cle: &str,
    groupe: &str,
    feeds: &[(&str, String)],
) -> anyhow::Result<()> {
    let url = format!("https://io.adafruit.com/api/v2/{username}/groups/{groupe}/data");
    let feeds_json = feeds
        .iter()
        .map(|(feed, valeur)| format!(r#"{{"key":"{feed}","value":"{valeur}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let corps = format!(r#"{{"feeds":[{feeds_json}]}}"#);
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
        info!(
            "Adafruit IO : envoi groupé réussi ({} feed(s), HTTP {statut})",
            feeds.len()
        );
    } else {
        warn!("Adafruit IO : réponse inattendue pour l'envoi groupé (HTTP {statut})");
    }

    Ok(())
}
