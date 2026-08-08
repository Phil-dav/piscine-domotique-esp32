//! Chien de garde réseau : vérifie que la carte est réellement **joignable**,
//! et pas seulement « associée » au point d'accès Wi-Fi.
//!
//! Contexte — incident du 08/08/2026. `is_connected()` d'esp-idf-svc ne reflète
//! que l'association 802.11 : c'est un simple drapeau alimenté par les
//! événements `STA_CONNECTED` / `STA_DISCONNECTED`, il ne prouve à aucun moment
//! qu'un paquet arrive à destination. Ce jour-là la carte est restée 1h30
//! associée, avec une IP valide (`sta ip: 192.168.1.20` dans le journal), tout
//! en étant totalement injoignable : `getaddrinfo() returns 202` côté Adafruit
//! IO, `ping` sans réponse depuis le PC (« hôte de destination injoignable »,
//! donc échec dès la résolution ARP) et dashboard mort.
//!
//! Le piège est que la logique de reconnexion de `main.rs` est gardée par
//! `if wifi_connecte { ... } else { ...reconnexion... }` : tant que
//! `is_connected()` répond `true`, **aucun mécanisme de secours ne peut se
//! déclencher**. Seul un redémarrage manuel a rétabli le service.
//!
//! D'où cette sonde active : un ping périodique de la passerelle (la box) est
//! le seul moyen de distinguer « associé et fonctionnel » de « associé et
//! mort ». Le ping vise la passerelle et pas un serveur sur Internet : on veut
//! tester le lien local, pas la disponibilité d'un service distant.
//!
//! La sonde tourne dans un **fil dédié** : `EspPing::ping` est bloquant, et un
//! appel réseau bloquant dans la boucle principale a déjà provoqué de vrais
//! redémarrages (voir la mémoire `projet-piege-appels-reseau-bloquants`).

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use esp_idf_svc::ping::{Configuration as ConfigPing, EspPing};
use log::{info, warn};
use parking_lot::Mutex;

/// Intervalle entre deux sondages. Assez espacé pour rester négligeable face au
/// trafic du dashboard, assez fréquent pour détecter un blocage en quelques
/// minutes plutôt qu'en une heure et demie.
const INTERVALLE_SONDE: Duration = Duration::from_secs(60);

/// Nombre de paquets par sondage. Deux suffisent : un seul rendrait la sonde
/// nerveuse au moindre paquet perdu (courant sur un signal à -70 dBm).
const PAQUETS_PAR_SONDE: u32 = 2;

/// `esp_ping` crée sa propre tâche FreeRTOS pour l'émission ICMP ; ce fil-ci ne
/// fait qu'attendre le résultat, il n'a donc pas besoin d'une grande pile.
const TAILLE_PILE: usize = 6144;

/// Poignée vers le fil de sondage. Toutes les méthodes sont non bloquantes.
pub struct Sonde {
    partage: Arc<Partage>,
}

struct Partage {
    /// Passerelle à sonder. Fournie par la boucle principale à partir de la
    /// configuration IP réelle plutôt que codée en dur : elle change si la
    /// carte est déplacée sur un autre réseau.
    passerelle: Mutex<Option<Ipv4Addr>>,
    /// Instant du dernier sondage réussi. `None` tant qu'aucun n'a abouti.
    dernier_succes: Mutex<Option<Instant>>,
}

impl Sonde {
    /// Démarre le fil de sondage. À appeler une seule fois, au démarrage.
    /// Tant qu'aucune passerelle n'est renseignée, le fil ne fait rien.
    pub fn demarrer() -> anyhow::Result<Self> {
        let partage = Arc::new(Partage {
            passerelle: Mutex::new(None),
            dernier_succes: Mutex::new(None),
        });

        let pour_fil = partage.clone();
        std::thread::Builder::new()
            .name("sonde_reseau".into())
            .stack_size(TAILLE_PILE)
            .spawn(move || boucle_sonde(pour_fil))?;

        Ok(Sonde { partage })
    }

    /// Renseigne (ou met à jour) la passerelle à surveiller.
    pub fn definir_passerelle(&self, passerelle: Ipv4Addr) {
        let mut courante = self.partage.passerelle.lock();
        if *courante != Some(passerelle) {
            info!("Sonde réseau : surveillance de la passerelle {passerelle}");
            *courante = Some(passerelle);
        }
    }

    /// Temps écoulé depuis le dernier sondage réussi.
    ///
    /// `None` signifie qu'**aucun** sondage n'a jamais abouti depuis le
    /// démarrage. C'est volontairement distinct de « ça fait longtemps » :
    /// l'appelant doit s'en servir comme garde-fou (une passerelle qui refuse
    /// l'ICMP ne doit surtout pas déclencher de redémarrages en boucle).
    pub fn depuis_dernier_succes(&self) -> Option<Duration> {
        self.partage.dernier_succes.lock().map(|t| t.elapsed())
    }
}

fn boucle_sonde(partage: Arc<Partage>) {
    // `esp-idf-svc` journalise chaque étape d'un ping en `info!` (une demi-douzaine
    // de lignes par sondage). Ce serait noyer les journaux que l'utilisateur relit
    // pour diagnostiquer. Best-effort : si le filtre ne prend pas, on récolte du
    // bruit dans les logs, jamais un défaut de fonctionnement.
    unsafe {
        esp_idf_svc::sys::esp_log_level_set(
            c"esp_idf_svc::ping".as_ptr(),
            esp_idf_svc::sys::esp_log_level_t_ESP_LOG_WARN,
        );
    }

    let conf = ConfigPing {
        count: PAQUETS_PAR_SONDE,
        interval: Duration::from_millis(500),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };

    loop {
        std::thread::sleep(INTERVALLE_SONDE);

        // Le verrou est relâché avant le ping (qui bloque plusieurs secondes) :
        // `definir_passerelle` ne doit jamais attendre après la boucle principale.
        let passerelle = *partage.passerelle.lock();
        let Some(passerelle) = passerelle else {
            continue;
        };

        match EspPing::new(0).ping(passerelle, &conf) {
            Ok(resume) if resume.received > 0 => {
                *partage.dernier_succes.lock() = Some(Instant::now());
            }
            Ok(resume) => warn!(
                "Sonde réseau : passerelle {passerelle} muette ({} paquet(s) envoyé(s), 0 reçu)",
                resume.transmitted
            ),
            Err(e) => warn!("Sonde réseau : échec du sondage de {passerelle} : {e:?}"),
        }
    }
}
