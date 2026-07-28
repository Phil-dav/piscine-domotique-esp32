use parking_lot::Mutex;
use std::sync::Arc;

/// Toutes les valeurs mesurées ou calculées, partagées entre
/// la boucle de lecture des capteurs et le serveur web.
#[derive(Clone, Debug)]
pub struct EtatCapteurs {
    // --- Capteurs ---
    pub temperature_air: Option<f32>, // None si l'AHT10 ne répond pas
    pub humidite: Option<f32>,
    pub temperature_eau: Option<f32>, // None si le DS18B20 n'a jamais répondu depuis le démarrage
    pub sonde_eau_muette: bool, // aucune lecture valide depuis plusieurs minutes (valeur affichée figée)
    pub sonde_eau_age_s: Option<u32>, // secondes depuis la dernière lecture valide

    // --- Wi-Fi ---
    pub wifi_connecte: bool,
    pub wifi_rssi: Option<i32>, // dBm ; None tant que non connecté
    pub wifi_ip: Option<String>,

    // --- GPS ---
    pub gps_ok: bool, // fix frais (< 5 s) avec >= 4 satellites
    pub gps_satellites: u8,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,

    // --- Horloge de l'automate (GPS en priorité, NTP/Wi-Fi en secours) ---
    pub heure_automate: Option<String>, // "JJ/MM/AAAA HH:MM:SS", heure locale Europe/Paris

    // --- Pompe / mode ---
    pub pompe_active: bool,
    pub pompe_bloquee: bool, // anti-claquement : blocage temporaire suite à trop de transitions
    pub pompe_heures_aujourdhui: f32,
    pub demande_pompe_manuelle: bool, // intention MARCHE/ARRÊT via la page web, appliquée seulement en mode MANU
    pub niveau_eau_ok: bool,
    pub mode: Mode,

    // --- Défauts / protections ---
    pub defaut_moteur: bool,
    pub defaut_moteur_verrouille: bool,
    pub demande_rearmement_moteur: bool, // mis à `true` par la route web, consommé par la boucle principale
    pub anti_gel: bool,
    pub canicule: bool,

    // --- Marche forcée (boost) — disponible en mode AUTO uniquement ---
    pub boost_actif: bool,
    pub boost_restant_secondes: u32,
    pub boost_marche_forcee: bool,
    pub boost_duree_minutes: u32,
    pub demande_boost_start: bool, // mis à `true` par la route web, consommé par la boucle principale
    pub demande_boost_stop: bool,
    pub demande_boost_duree: Option<u32>,

    // --- Filtration automatique (mode AUTO) ---
    pub filt_objectif_heures: f32, // heures de filtration cibles aujourd'hui, selon la température
    pub filt_debut_effectif: f32,  // plage horaire effective (heures décimales), selon le régime actif
    pub filt_fin_effective: f32,
    pub filt_debut_configure: f32, // plage horaire standard configurée (indépendante du régime actif)
    pub filt_fin_configuree: f32,
    pub demande_plage: Option<(f32, f32)>, // mis par la route web, consommé par la boucle principale

    // --- Historique des modes (timeline colorée du dashboard) ---
    pub historique_modes: Vec<crate::historique_modes::Segment>,

    // --- Batterie / solaire (voir batterie.rs) ---
    pub tension_batterie_v: Option<f32>, // MTB 12 : bornes de la batterie
    pub tension_sortie_5v_v: Option<f32>, // MTB 5 : sortie régulée du contrôleur solaire
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Auto,
    Manuel,
    Off,
}

impl Default for EtatCapteurs {
    fn default() -> Self {
        EtatCapteurs {
            temperature_air: None,
            humidite: None,
            temperature_eau: None,
            sonde_eau_muette: false,
            sonde_eau_age_s: None,
            wifi_connecte: false,
            wifi_rssi: None,
            wifi_ip: None,
            gps_ok: false,
            gps_satellites: 0,
            gps_latitude: None,
            gps_longitude: None,
            heure_automate: None,
            pompe_active: false,
            pompe_bloquee: false,
            pompe_heures_aujourdhui: 0.0,
            demande_pompe_manuelle: false,
            niveau_eau_ok: true,
            mode: Mode::Off,
            defaut_moteur: false,
            defaut_moteur_verrouille: false,
            demande_rearmement_moteur: false,
            anti_gel: false,
            canicule: false,
            boost_actif: false,
            boost_restant_secondes: 0,
            boost_marche_forcee: false,
            boost_duree_minutes: 60,
            demande_boost_start: false,
            demande_boost_stop: false,
            demande_boost_duree: None,
            filt_objectif_heures: 0.0,
            filt_debut_effectif: 8.0,
            filt_fin_effective: 20.0,
            filt_debut_configure: 8.0,
            filt_fin_configuree: 20.0,
            demande_plage: None,
            historique_modes: Vec::new(),
            tension_batterie_v: None,
            tension_sortie_5v_v: None,
        }
    }
}

/// Type pratique : un état partageable entre plusieurs parties du programme.
pub type EtatPartage = Arc<Mutex<EtatCapteurs>>;

/// Crée un nouvel état partagé, avec des valeurs par défaut.
pub fn nouveau() -> EtatPartage {
    Arc::new(Mutex::new(EtatCapteurs::default()))
}
