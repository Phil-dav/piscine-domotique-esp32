mod adafruit_io;
mod aht10;
mod batterie;
mod boost;
mod config;
mod ds18b20;
mod ecran;
mod etat_partage;
mod filtration_auto;
mod gps;
mod historique_modes;
mod journal;
mod niveau_eau;
mod pcf8574;
mod pompe;
mod securite;
mod stockage;
mod temps;
mod web_server;
mod wifi;

use chrono::{Datelike, Timelike};
use core::cell::RefCell;
use embedded_hal_bus::i2c::RefCellDevice;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::adc::oneshot::{
    config::{AdcChannelConfig, Calibration},
    AdcChannelDriver, AdcDriver,
};
use esp_idf_svc::hal::adc::{attenuation, Resolution};
use esp_idf_svc::hal::delay::{Delay, FreeRtos};
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::reset::ResetReason;
use esp_idf_svc::hal::task::watchdog;
use esp_idf_svc::hal::uart::{config::Config as UartConfig, UartDriver};
use esp_idf_svc::hal::units::{FromValueType, Hertz};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::{BlockingWifi, EspWifi};
use log::{info, warn};
use one_wire_bus::OneWire;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};
use std::time::{Duration, Instant};

const CLE_BOOST_MIN: &str = "boost_min";
const CLE_DERNIER_HORODATAGE: &str = "dernier_ts";
/// En dessous de ce seuil, un redémarrage juste après une coupure d'alimentation
/// (POWERON/BROWNOUT) ressemble à une micro-coupure secteur plutôt qu'à un arrêt
/// volontaire prolongé — seuil choisi par l'utilisateur, ajustable ici si besoin.
const SEUIL_COUPURE_COURTE_MIN: i64 = 2;

/// Traduit la raison de redémarrage ESP-IDF en texte court pour le journal des alertes.
fn raison_reset_texte(raison: ResetReason) -> &'static str {
    match raison {
        ResetReason::PowerOn => "ALIMENTATION",
        ResetReason::Brownout => "SOUS_TENSION",
        ResetReason::Panic => "PANIC",
        ResetReason::TaskWatchdog => "WATCHDOG_TACHE",
        ResetReason::InterruptWatchdog => "WATCHDOG_INTERRUPTION",
        ResetReason::Watchdog => "WATCHDOG",
        ResetReason::Software => "LOGICIEL",
        ResetReason::ExternalPin => "BOUTON_RESET",
        ResetReason::DeepSleep => "REVEIL_VEILLE",
        ResetReason::Unknown => "INCONNUE",
        _ => "AUTRE",
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    if !config::LOGS_SERIE_ACTIFS {
        log::set_max_level(log::LevelFilter::Off);
    }

    // Capturée le plus tôt possible : c'est la seule fenêtre où ESP-IDF garantit que
    // cette info reflète encore le redémarrage qui vient de se produire.
    let raison_reset = ResetReason::get();

    info!("Démarrage du programme");

    let debut_programme = Instant::now();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let mut stockage = stockage::Stockage::ouvrir(nvs.clone())?;
    // Journaux (sessions pompe, bilans, alertes) : historique borné persisté en NVS,
    // partagé entre la boucle principale (écriture) et le serveur web (lecture/effacement).
    let journal = std::sync::Arc::new(parking_lot::Mutex::new(journal::Journal::ouvrir(
        nvs.clone(),
    )?));

    // --- I2C (AHT10 + écran + PCF8574) ---
    // 400 kHz (Fast Mode) au lieu de 100 kHz : les 3 périphériques le supportent, et ça
    // réduit d'autant le temps de blocage de chaque transaction I2C — notamment le flush
    // de l'écran OLED (~1 Ko de données), qui est la plus grosse transaction du bus.
    let sda = peripherals.pins.gpio21;
    let scl = peripherals.pins.gpio22;
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &i2c_config)?;
    let i2c_partage = RefCell::new(i2c);

    // --- 1-Wire (DS18B20) ---
    let one_wire_pin = PinDriver::input_output_od(peripherals.pins.gpio4, Pull::Up)?;
    let mut one_wire =
        OneWire::new(one_wire_pin).map_err(|e| anyhow::anyhow!("Erreur init 1-Wire : {:?}", e))?;
    let mut delay = Delay::new_default();

    // --- GPS (UART2, trames NMEA) ---
    // Tampon RX agrandi (défaut = 256 octets) : le module GPS émet en continu, mais
    // `gps.mettre_a_jour()` n'est appelé qu'une fois par itération de la boucle principale,
    // qui peut prendre 2-3 s (conversion DS18B20, delay final...). Avec un tampon trop petit,
    // ces quelques secondes suffisent à le faire déborder et à corrompre des trames NMEA.
    let gps_uart_config = UartConfig::default()
        .baudrate(Hertz(config::GPS_BAUDRATE))
        .rx_fifo_size(2048);
    let gps_uart = UartDriver::new(
        peripherals.uart2,
        peripherals.pins.gpio17, // TX (non utilisé, le module GPS n'écoute rien)
        peripherals.pins.gpio16, // RX
        Option::<AnyIOPin>::None,
        Option::<AnyIOPin>::None,
        &gps_uart_config,
    )?;
    let mut gps = gps::Gps::new(gps_uart);

    // --- Sécurité pompe : niveau d'eau (GPIO34, ADC1) ---
    // GPIO33 (ex-feedback relais miroir) est libre : réservé au futur capteur de courant.
    let adc1 = AdcDriver::new(peripherals.adc1)?;
    let niveau_config = AdcChannelConfig {
        attenuation: attenuation::DB_12,
        resolution: Resolution::Resolution12Bit,
        calibration: Default::default(),
    };
    let mut niveau_chan = AdcChannelDriver::new(&adc1, peripherals.pins.gpio34, &niveau_config)?;

    // --- Ponts diviseurs batterie/solaire (voir batterie.rs et la mémoire
    // projet-alimentation-batterie-solaire pour le détail des résistances) ---
    // Calibration::Line (basée sur les eFuses de la puce) au lieu de None : ces
    // deux lectures servent à suivre une vraie tension, contrairement au niveau
    // d'eau qui n'est qu'un seuil, donc la précision compte davantage ici.
    let batterie_config = AdcChannelConfig {
        attenuation: attenuation::DB_12,
        resolution: Resolution::Resolution12Bit,
        calibration: Calibration::Line,
    };
    let mut mtb12_chan = AdcChannelDriver::new(&adc1, peripherals.pins.gpio32, &batterie_config)?;
    let mut mtb5_chan = AdcChannelDriver::new(&adc1, peripherals.pins.gpio35, &batterie_config)?;

    FreeRtos::delay_ms(100);

    let mut sorties = pcf8574::init(&i2c_partage)?;

    let interface = I2CDisplayInterface::new(RefCellDevice::new(&i2c_partage));
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    // L'écran est un confort de diagnostic, pas une fonction vitale : s'il refuse de
    // s'initialiser (bus I2C pas encore stable à froid, écran pas alimenté à temps...),
    // le programme continue quand même sans lui plutôt que de tout arrêter comme avant
    // (Wi-Fi et pompe doivent fonctionner même si l'écran a un problème).
    let mut ecran_disponible = false;
    for tentative in 1..=5 {
        match display.init() {
            Ok(()) => {
                ecran_disponible = true;
                break;
            }
            Err(e) => {
                warn!("Écran OLED : échec init (essai {}/5) : {:?}", tentative, e);
                FreeRtos::delay_ms(200);
            }
        }
    }
    if !ecran_disponible {
        warn!("Écran OLED indisponible — le programme continue sans affichage.");
    }

    // --- Wi-Fi ---
    if ecran_disponible {
        if let Err(e) = ecran::afficher_connexion(&mut display) {
            warn!("Écran OLED : erreur d'affichage (ignorée) : {:?}", e);
        } else if let Err(e) = display.flush() {
            warn!("Écran OLED : erreur de flush (ignorée) : {:?}", e);
        }
    }

    // --- État partagé (toujours créé, utilisé même sans Wi-Fi pour piloter la pompe) ---
    let etat = etat_partage::nouveau();

    let mut wifi: Option<BlockingWifi<EspWifi<'static>>> = None;
    let mut ip_texte = "Wi-Fi désactivé (test)".to_string();
    // `_sntp`/`_server` doivent rester en vie tout le programme (sinon ils s'arrêtent) :
    // liés ici pour ne pas être supprimés à la fin de ce bloc.
    let mut sntp = None;
    let mut _server = None;

    if config::WIFI_ACTIF {
        let w = wifi::connecter(peripherals.modem, sys_loop, nvs)?;
        // Synchronisation horaire de secours (utilisée seulement si le GPS n'a pas de fix frais).
        sntp = Some(EspSntp::new_default()?);

        let ip_info = w.wifi().sta_netif().get_ip_info()?;
        ip_texte = ip_info.ip.to_string();

        if ecran_disponible {
            if let Err(e) =
                ecran::afficher_connecte(&mut display, &ip_texte, w.wifi().get_rssi().ok())
            {
                warn!("Écran OLED : erreur d'affichage (ignorée) : {:?}", e);
            } else if let Err(e) = display.flush() {
                warn!("Écran OLED : erreur de flush (ignorée) : {:?}", e);
            }
        }

        {
            let mut donnees = etat.lock();
            donnees.wifi_connecte = true;
            donnees.wifi_ip = Some(ip_texte.clone());
        }
        _server = Some(web_server::demarrer(etat.clone(), journal.clone())?);
        wifi = Some(w);
    } else {
        warn!("Wi-Fi désactivé (config::WIFI_ACTIF = false) : test temporaire, pas de serveur web, pas de synchro NTP, heure GPS uniquement.");
    }

    // Dernières valeurs valides : conservées telles quelles en cas d'échec de lecture
    // ponctuel, plutôt que d'afficher une valeur manquante côté dashboard.
    let mut derniere_temperature_air: Option<f32> = None;
    let mut derniere_humidite: Option<f32> = None;

    // AHT10 : bloque ~80 ms à chaque lecture (temps de mesure du capteur). On ne
    // l'interroge donc que toutes les 2 s au lieu de chaque tick, pour ne pas ralentir
    // la réactivité du bouton écran.
    const INTERVALLE_AHT10: Duration = Duration::from_secs(2);
    let mut dernier_aht10 = Instant::now() - INTERVALLE_AHT10;

    let mut sonde_eau = ds18b20::SondeEau::nouveau();

    let mut securite_moteur = securite::SecuriteMoteur::nouveau();
    let mut niveau_eau = niveau_eau::NiveauEau::nouveau();
    let mut pompe = pompe::GestionPompe::nouveau();
    let mut filtration_auto = filtration_auto::FiltrationAuto::nouveau(&stockage);
    let mut boost = boost::Boost::nouveau(stockage.lire_u32(CLE_BOOST_MIN, 60));
    let mut dernier_jour: Option<u32> = None;
    let mut dernier_jour_texte: Option<String> = None;
    // Verrou de sécurité : une fois l'objectif d'heures du jour atteint, la filtration AUTO
    // ne redémarre plus tant que le jour n'a pas changé — même si `heures_cibles` remonte
    // ensuite suite à une variation de température (ex. chute brutale lors d'un orage), pour
    // éviter tout cyclage marche/arrêt répété sur le relais pompe.
    let mut filtration_terminee_aujourdhui = false;

    // Journal : suivi des sessions pompe (début/fin) et des alertes (front montant
    // de chaque condition), pour alimenter le bilan journalier.
    let mut session_debut: Option<(String, Instant)> = None;
    let mut pompe_precedent = false;
    // Détecte un changement d'état pompe pendant le bloc journal (verrouillé) ;
    // consommé juste après, une fois le verrou relâché, pour envoyer vers Adafruit IO
    // sans jamais bloquer le journal pendant l'appel réseau.
    let mut pompe_transition: Option<bool> = None;
    // Même principe que `pompe_transition`, pour le mode (AUTO/MANU/OFF) : détecté
    // près de `historique_modes`, consommé plus loin vers Adafruit IO une fois le
    // Wi-Fi connu.
    let mut mode_transition: Option<etat_partage::Mode> = None;
    let mut niveau_ok_precedent = true;
    let mut securite_actif_precedent = false;
    let mut sonde_muette_precedente = false;
    let mut pompe_bloquee_precedent = false;
    let mut anti_gel_precedent = false;
    let mut canicule_precedent = false;
    let mut alertes_jour: u32 = 0;

    // Redémarrage : journalisé une seule fois, dès que l'heure locale est fiable
    // (nécessaire pour estimer la durée d'arrêt via l'horodatage sauvegardé en NVS).
    let dernier_horodatage_connu = stockage.lire_u32(CLE_DERNIER_HORODATAGE, 0);
    let mut redemarrage_journalise = false;
    let mut dernier_sauvegarde_horodatage = Instant::now() - Duration::from_secs(60);
    let mut dernier_sauvegarde_pompe_jour = Instant::now() - Duration::from_secs(60);

    // Historique coloré des modes pour la timeline du dashboard (portage de ModeHistory).
    let mut historique = historique_modes::HistoriqueModes::nouveau();
    let mut historique_initialise = false;
    let mut dernier_mode_physique = etat_partage::Mode::Off;
    // Dernier type de segment de timeline ouvert (0=OFF, 1=AUTO, 2=MANU_ON, 3=MANU_OFF,
    // 4=BOOST_ON, 5=BOOST_OFF) — sert à détecter tout changement (mode, marche/arrêt en
    // Manuel, ou Boost) qui doit fermer le segment en cours et en ouvrir un nouveau.
    let mut dernier_type_segment: Option<u8> = None;

    // Second historique, séparé du premier : la marche RÉELLE de la pompe (0=arrêtée,
    // 1=en marche), indépendamment du mode sélectionné — sert à tracer un trait fidèle
    // sur le dashboard plutôt que le bloc synthétique "Fait" actuel, toujours ancré à
    // l'heure de début de plage (voir l'échange du 29/07/2026 : un boost ayant tourné
    // cette nuit-là apparaissait à tort comme si la pompe avait tourné dès 8h00).
    let mut historique_pompe = historique_modes::HistoriqueModes::nouveau();
    let mut dernier_etat_pompe_segment: Option<u8> = None;

    // Écran OLED : 5 pages, bouton P4 pour naviguer.
    // 0=capteurs, 1=Wi-Fi, 2=pompe/filtration, 3=sécurités/uptime, 4=bilan journalier.
    const NB_PAGES: u8 = 5;
    const RETOUR_AUTO_APRES: Duration = Duration::from_secs(30);
    const APPUI_LONG: Duration = Duration::from_millis(800);
    const VEILLE_APRES: Duration = Duration::from_secs(300);

    let mut page_ecran: u8 = 0;
    let mut bouton_precedent = false;
    let mut appui_depuis: Option<Instant> = None;
    let mut appui_traite = false;
    let mut dernier_changement_page = Instant::now();
    let mut derniere_activite = Instant::now();
    let mut ecran_en_veille = false;
    let mut alerte_visible = true;
    let mut dernier_clignotement = Instant::now();
    let mut temp_min_jour: Option<f32> = None;
    let mut temp_max_jour: Option<f32> = None;

    // Redessin de l'écran : seulement quand quelque chose a changé (page, alerte,
    // veille, clignotement) ou au bout d'un intervalle périodique — le flush I2C du
    // framebuffer est la plus grosse transaction du bus, inutile de la refaire à chaque tick.
    const INTERVALLE_DESSIN: Duration = Duration::from_millis(500);
    let mut page_ecran_precedente: u8 = u8::MAX; // force le tout premier dessin
    let mut alerte_precedente = false;
    let mut veille_precedente = false;
    let mut dernier_dessin = Instant::now() - INTERVALLE_DESSIN;

    // Le journal série n'a pas besoin d'être mis à jour à chaque tick.
    const INTERVALLE_LOG: Duration = Duration::from_secs(2);
    let mut dernier_log = Instant::now() - INTERVALLE_LOG;

    // Reconnexion Wi-Fi : vérifiée toutes les 30 s, comme le projet C++ de référence.
    // `connect()` sur l'objet Wi-Fi non bloquant (via `wifi_mut()`) se contente de
    // déclencher la tentative (`esp_wifi_connect`) et revient tout de suite — la
    // connexion réelle se fait en tâche de fond, sans jamais geler la boucle principale.
    const INTERVALLE_WIFI: Duration = Duration::from_secs(30);
    let mut dernier_controle_wifi = Instant::now();

    // Envoi périodique des mesures vers Adafruit IO (cloud gratuit) : toutes les 5
    // minutes suffit largement (l'eau/l'air ne varient pas vite).
    const INTERVALLE_ENVOI_CLOUD: Duration = Duration::from_secs(300);
    let mut dernier_envoi_periodique = Instant::now();

    // Envois événementiels (pompe/mode) : marge de 16 s entre deux envois pour rester
    // raisonnable côté réseau. Si un changement survient trop tôt, il n'est pas perdu :
    // il reste en attente (`pompe_transition`/`mode_transition` non consommés) et part
    // dès que le délai est écoulé, avec la valeur la plus récente.
    const DELAI_MIN_EVENEMENT: Duration = Duration::from_secs(16);
    let mut dernier_envoi_evenement = Instant::now() - DELAI_MIN_EVENEMENT;

    // Fil d'envoi Adafruit IO dédié : la boucle principale ne fait qu'y déposer des
    // mesures (opération instantanée), elle ne fait jamais l'appel réseau elle-même.
    // Voir l'explication détaillée en tête de `adafruit_io.rs` — un appel HTTPS peut
    // bloquer bien au-delà du délai configuré sur un Wi-Fi qui perd des paquets, ce
    // qui déclenchait le Task Watchdog et redémarrait la carte.
    let expediteur_adafruit = adafruit_io::Expediteur::demarrer(
        config::ADAFRUIT_IO_USERNAME,
        config::ADAFRUIT_IO_KEY,
        config::ADAFRUIT_IO_GROUP_KEY,
    )?;

    // Task Watchdog : si la boucle principale ne "nourrit" plus le chien de garde
    // pendant `CONFIG_ESP_TASK_WDT_TIMEOUT_S` (voir sdkconfig.defaults), ESP-IDF
    // déclenche un panic -> redémarrage automatique, journalisé comme un
    // `REDEMARRAGE` de type `WATCHDOG_TACHE` (voir raison_reset_texte). Transforme un
    // gel silencieux (débranchement physique nécessaire) en incident diagnostiqué.
    let twdt_config = watchdog::TWDTConfig::new();
    let mut twdt_driver = watchdog::TWDTDriver::new(peripherals.twdt, &twdt_config)?;
    let mut watchdog = twdt_driver.watch_current_task()?;

    loop {
        watchdog.feed()?;

        if dernier_aht10.elapsed() >= INTERVALLE_AHT10 {
            dernier_aht10 = Instant::now();
            match aht10::lire_temperature_humidite(&i2c_partage) {
                Ok((t, h)) => {
                    derniere_temperature_air = Some(t);
                    derniere_humidite = Some(h);
                }
                Err(e) => warn!(
                    "AHT10 : lecture échouée, dernière valeur conservée : {:?}",
                    e
                ),
            }
        }

        let derniere_temperature_eau = sonde_eau.mettre_a_jour(&mut one_wire, &mut delay);
        let sonde_muette = sonde_eau.muette();

        let mode_physique = match pcf8574::lire_mode(&i2c_partage) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "PCF8574 : lecture mode échouée, mode précédent conservé : {:?}",
                    e
                );
                etat.lock().mode.clone()
            }
        };
        // Réarmement manuel demandé depuis la page web ?
        let demande_reset = {
            let mut donnees = etat.lock();
            let demande = donnees.demande_rearmement_moteur;
            donnees.demande_rearmement_moteur = false;
            demande
        };
        if demande_reset {
            securite_moteur.rearmer();
            info!("Défaut moteur réarmé manuellement");
        }

        let defaut_brut = pcf8574::lire_defaut_moteur(&i2c_partage).unwrap_or(false);
        securite_moteur.mettre_a_jour(defaut_brut);

        // Bouton écran (P4) : appui court = page suivante, appui long (≥ 0,8 s) = retour
        // page 0, écran en veille = un relâchement réveille sans changer de page.
        // Nuance : la boucle principale ne scrute ce bouton qu'une fois toutes les 2-3 s,
        // donc la distinction court/long n'a pas la précision du C++ (qui scrute en continu) —
        // en pratique, un appui relâché avant le prochain passage en boucle est "court",
        // un appui encore actif au passage suivant est "long".
        let bouton_actif = pcf8574::lire_bouton_oled(&i2c_partage).unwrap_or(false);
        let maintenant = Instant::now();

        if bouton_actif && !bouton_precedent {
            appui_depuis = Some(maintenant);
            appui_traite = false;
        }

        if ecran_en_veille {
            if !bouton_actif && bouton_precedent {
                ecran_en_veille = false;
                derniere_activite = maintenant;
                appui_traite = true;
            }
        } else {
            if bouton_actif
                && !appui_traite
                && appui_depuis.is_some_and(|d| maintenant.duration_since(d) >= APPUI_LONG)
            {
                page_ecran = 0;
                dernier_changement_page = maintenant;
                derniere_activite = maintenant;
                appui_traite = true;
            }
            if !bouton_actif && bouton_precedent && !appui_traite {
                page_ecran = (page_ecran + 1) % NB_PAGES;
                dernier_changement_page = maintenant;
                derniere_activite = maintenant;
            }
        }
        bouton_precedent = bouton_actif;

        // Retour automatique à la page 0 après 30 s d'inactivité sur une autre page.
        if page_ecran != 0
            && maintenant.duration_since(dernier_changement_page) >= RETOUR_AUTO_APRES
        {
            page_ecran = 0;
        }

        // --- Horloge : GPS en priorité (fix frais), NTP/Wi-Fi en secours ---
        gps.mettre_a_jour()?;
        let gps_ok = gps.valide();
        let gps_satellites = gps.satellites();
        let gps_position = gps.position();

        let heure_utc = if gps_ok {
            gps.temps_utc()
        } else if sntp
            .as_ref()
            .is_some_and(|s| s.get_sync_status() == SyncStatus::Completed)
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
                .map(|dt| dt.naive_utc())
        } else {
            None
        };
        let heure_locale = heure_utc.map(temps::vers_heure_paris);
        let heure_automate = heure_locale.map(|h| h.format("%d/%m/%Y %H:%M:%S").to_string());
        // Pour le journal (sessions, alertes, bilan) : mêmes conventions que le C++
        // (date JJ/MM/AAAA, heure HH:MM:SS), "indisponible" tant que l'heure n'est pas fiable.
        let date_texte = heure_locale.map(|h| h.format("%d/%m/%Y").to_string());
        let heure_texte = heure_locale.map(|h| h.format("%H:%M:%S").to_string());

        // --- Redémarrage : journalisé une seule fois, dès que l'heure est fiable ---
        // Durée d'arrêt estimée par différence avec le dernier horodatage sauvegardé
        // (pas d'horloge qui tourne hors tension sur cet ESP32, donc pas d'autre moyen
        // de savoir combien de temps s'est écoulé pendant la coupure).
        if !redemarrage_journalise {
            if let Some(utc) = heure_utc {
                redemarrage_journalise = true;
                let epoch_actuel = utc.and_utc().timestamp();
                let raison_texte = raison_reset_texte(raison_reset);
                let valeur = if dernier_horodatage_connu == 0 {
                    format!("{raison_texte} (premier demarrage)")
                } else {
                    let duree_min = (epoch_actuel - dernier_horodatage_connu as i64).max(0) / 60;
                    let note = if duree_min < SEUIL_COUPURE_COURTE_MIN {
                        "coupure courte"
                    } else {
                        "arret prolonge"
                    };
                    format!("{raison_texte} ~{duree_min}min ({note})")
                };
                journal.lock().enregistrer_alerte(
                    date_texte.as_deref().unwrap_or("--/--/----"),
                    heure_texte.as_deref().unwrap_or("--:--:--"),
                    "REDEMARRAGE",
                    &valeur,
                );
                info!("Redémarrage journalisé : {}", valeur);
            }
        }
        if let Some(utc) = heure_utc {
            if dernier_sauvegarde_horodatage.elapsed() >= Duration::from_secs(60) {
                dernier_sauvegarde_horodatage = maintenant;
                stockage.ecrire_u32(CLE_DERNIER_HORODATAGE, utc.and_utc().timestamp() as u32);
            }
        }

        // --- Niveau d'eau + sécurité globale ---
        let niveau_brut = niveau_chan.read_raw().unwrap_or(0);
        let niveau_ok = niveau_eau.mettre_a_jour(niveau_brut);
        let systeme_sur = !securite_moteur.actif() && niveau_ok;

        // --- Tension batterie/solaire (ponts diviseurs, voir batterie.rs) ---
        let tension_batterie = batterie::lire_volts(&mut mtb12_chan, batterie::RATIO_MTB_12);
        let tension_sortie_5v = batterie::lire_volts(&mut mtb5_chan, batterie::RATIO_MTB_5);

        // --- Demandes reçues depuis la page web (boost, plage horaire) ---
        {
            let mut donnees = etat.lock();
            if let Some(minutes) = donnees.demande_boost_duree.take() {
                if boost.definir_duree(minutes) {
                    stockage.ecrire_u32(CLE_BOOST_MIN, minutes);
                    info!("Durée du boost réglée à {} min", minutes);
                }
            }
            if donnees.demande_boost_start {
                donnees.demande_boost_start = false;
                boost.demarrer(pompe.etat());
                info!(
                    "Marche forcée demandée ({}, {} min)",
                    if boost.marche_forcee() {
                        "MARCHE"
                    } else {
                        "ARRET"
                    },
                    boost.duree_minutes()
                );
            }
            if donnees.demande_boost_stop {
                donnees.demande_boost_stop = false;
                boost.arreter();
                info!("Marche forcée annulée");
            }
            if let Some((debut, fin)) = donnees.demande_plage.take() {
                if filtration_auto.definir_plage(&mut stockage, debut, fin) {
                    info!("Plage horaire modifiée : {:.1}h - {:.1}h", debut, fin);
                }
            }
        }

        // --- Filtration automatique : heures cibles du jour + plage horaire effective ---
        let heures_cibles = derniere_temperature_eau
            .map(|t| filtration_auto.heures_cibles(t))
            .unwrap_or(0.0);
        let (plage_debut, plage_fin) = filtration_auto.plage_horaire();

        // Remise à zéro du compteur de filtration au changement de jour (heure locale requise).
        if let Some(locale) = heure_locale {
            let jour = locale.day();
            if dernier_jour.is_some_and(|j| j != jour) {
                // Bilan du jour qui vient de se terminer, avant remise à zéro des compteurs.
                if let Some(date_bilan) = &dernier_jour_texte {
                    let mode_jour_texte = match &dernier_mode_physique {
                        etat_partage::Mode::Auto => "AUTO",
                        etat_partage::Mode::Manuel => "MANU",
                        etat_partage::Mode::Off => "OFF",
                    };
                    journal.lock().enregistrer_bilan(
                        date_bilan,
                        heures_cibles,
                        pompe.heures_aujourdhui(),
                        pompe.sessions_aujourdhui(),
                        alertes_jour,
                        mode_jour_texte,
                    );
                }
                alertes_jour = 0;
                pompe.reinitialiser_jour();
                filtration_terminee_aujourdhui = false;
                temp_min_jour = None;
                temp_max_jour = None;
                historique.reinitialiser();
                historique_pompe.reinitialiser();
                historique_initialise = false;
                info!(
                    "Nouveau jour ({}) : compteur de filtration remis à zéro",
                    jour
                );
            }
            dernier_jour = Some(jour);
            dernier_jour_texte = date_texte.clone();
        }

        // Suivi de la température eau min/max du jour (pour la page bilan).
        if let Some(t) = derniere_temperature_eau {
            temp_min_jour = Some(temp_min_jour.map_or(t, |m| m.min(t)));
            temp_max_jour = Some(temp_max_jour.map_or(t, |m| m.max(t)));
        }

        let heure_decimale = heure_locale
            .map(|h| h.hour() as f32 + h.minute() as f32 / 60.0 + h.second() as f32 / 3600.0)
            .unwrap_or(12.0); // pas d'heure fiable : valeur neutre, comme le projet C++
        let dans_plage = heure_decimale >= plage_debut && heure_decimale < plage_fin;

        // Historique des modes pour la timeline du dashboard : un segment n'est ouvert
        // que si l'heure locale est fiable (comme le C++, qui exige getDecimalHour() >= 0).
        // Type de segment calculé à partir de trois informations combinées (mode, pompe,
        // boost), pour que les 6 couleurs de la légende (OFF, AUTO, MANU ON/OFF,
        // BOOST ON/OFF) correspondent vraiment à quelque chose — avant ce correctif,
        // Manuel restait toujours codé "MANU OFF" et le Boost n'avait jamais sa propre
        // couleur (voir [[projet-ambiguite-date-session-minuit]] et l'échange du
        // 29/07/2026 sur le mélange manuel/pompe dans la timeline).
        let type_segment_actuel = |mode: &etat_partage::Mode,
                                   pompe_on: bool,
                                   boost_actif: bool,
                                   boost_marche_forcee: bool|
         -> u8 {
            if boost_actif {
                if boost_marche_forcee {
                    4
                } else {
                    5
                } // BOOST ON / BOOST OFF
            } else {
                match mode {
                    etat_partage::Mode::Auto => 1,
                    etat_partage::Mode::Manuel => {
                        if pompe_on {
                            2 // MANU ON
                        } else {
                            3 // MANU OFF
                        }
                    }
                    etat_partage::Mode::Off => 0,
                }
            }
        };
        let type_actuel = type_segment_actuel(
            &mode_physique,
            pompe.etat(),
            boost.actif(),
            boost.marche_forcee(),
        );
        if mode_physique != dernier_mode_physique {
            mode_transition = Some(mode_physique.clone());
        }
        if let Some(date_du_jour) = date_texte.as_deref() {
            if !historique_initialise {
                // Après un redémarrage, essaie de retrouver la timeline du jour en
                // cours dans la NVS plutôt que de repartir de zéro (voir
                // [[projet-cloud-thingspeak-alertes]] : la barre "Phase
                // d'optimisation active" était jusqu'ici perdue à chaque reset).
                let segments_recharges = journal.lock().charger_timeline(date_du_jour);
                if segments_recharges.is_empty() {
                    historique.demarrer(type_actuel, heure_decimale);
                    dernier_type_segment = Some(type_actuel);
                } else {
                    let nb_segments = segments_recharges.len();
                    // Réaligne le suivi du type de segment avec le dernier segment
                    // rechargé, pour ne pas ouvrir un doublon si rien n'a changé depuis
                    // le redémarrage (`dernier_mode_physique`, lui, est de toute façon
                    // réécrit à chaque tour de boucle avec le mode réel du moment, plus
                    // bas — inutile de le réaligner ici).
                    if let Some(dernier) = segments_recharges.last() {
                        dernier_type_segment = Some(dernier.type_segment);
                    }
                    historique =
                        historique_modes::HistoriqueModes::depuis_segments(segments_recharges);
                    info!("Timeline du jour rechargée depuis la NVS ({nb_segments} segment(s))");
                }
                historique_initialise = true;
                journal
                    .lock()
                    .sauvegarder_timeline(date_du_jour, historique.segments());

                // Même principe pour le second historique (marche réelle de la pompe).
                let etat_pompe_actuel = u8::from(pompe.etat());
                let segments_pompe_recharges = journal.lock().charger_timeline_pompe(date_du_jour);
                if segments_pompe_recharges.is_empty() {
                    historique_pompe.demarrer(etat_pompe_actuel, heure_decimale);
                    dernier_etat_pompe_segment = Some(etat_pompe_actuel);
                } else {
                    if let Some(dernier) = segments_pompe_recharges.last() {
                        dernier_etat_pompe_segment = Some(dernier.type_segment);
                    }
                    historique_pompe = historique_modes::HistoriqueModes::depuis_segments(
                        segments_pompe_recharges,
                    );
                }
                journal
                    .lock()
                    .sauvegarder_timeline_pompe(date_du_jour, historique_pompe.segments());

                // Même principe pour le temps de marche / nombre de sessions du jour
                // (voir pompe.rs) : sans ça, ces compteurs repartaient de zéro à
                // chaque redémarrage alors que la timeline juste au-dessus, elle,
                // survivait déjà — incohérence relevée par l'utilisateur le
                // 28/07/2026 en comparant la barre et le compteur "Fait".
                if let Some((heures, sessions)) = journal.lock().charger_pompe_jour(date_du_jour) {
                    pompe.charger_etat_jour(heures, sessions);
                    info!("Compteurs pompe du jour rechargés depuis la NVS ({heures:.2}h, {sessions} session(s))");
                }
            }
            // Un seul déclencheur pour toute la timeline : dès que le type de segment
            // calculé plus haut change (mode, marche/arrêt en Manuel, ou début/fin de
            // Boost), on ferme le segment en cours et on en ouvre un nouveau.
            if dernier_type_segment != Some(type_actuel) {
                historique.demarrer(type_actuel, heure_decimale);
                journal
                    .lock()
                    .sauvegarder_timeline(date_du_jour, historique.segments());
                dernier_type_segment = Some(type_actuel);
            }

            // Second déclencheur, indépendant du premier : la marche réelle de la
            // pompe, pour le tracé bleu clair du dashboard.
            let etat_pompe_actuel = u8::from(pompe.etat());
            if dernier_etat_pompe_segment != Some(etat_pompe_actuel) {
                historique_pompe.demarrer(etat_pompe_actuel, heure_decimale);
                journal
                    .lock()
                    .sauvegarder_timeline_pompe(date_du_jour, historique_pompe.segments());
                dernier_etat_pompe_segment = Some(etat_pompe_actuel);
            }
        }
        dernier_mode_physique = mode_physique.clone();

        let mode_texte = match &mode_physique {
            etat_partage::Mode::Auto => "AUTO",
            etat_partage::Mode::Manuel => "MANU",
            etat_partage::Mode::Off => "OFF",
        };

        // --- Décision de pilotage pompe ---
        let demande_manuelle = etat.lock().demande_pompe_manuelle;
        let demande_pompe = if derniere_temperature_eau.is_some_and(|t| t < 2.0) {
            // Anti-gel absolu : priorité totale, même hors mode AUTO (y compris en OFF).
            true
        } else {
            match &mode_physique {
                etat_partage::Mode::Manuel => demande_manuelle,
                etat_partage::Mode::Auto => {
                    if boost.actif() {
                        boost.marche_forcee()
                    } else {
                        // Le verrou ne s'arme que sur un objectif réellement calculé : au
                        // démarrage la sonde DS18B20 n'a pas encore répondu (conversion
                        // ~750 ms, non bloquante), `heures_cibles` vaut alors 0.0 par défaut
                        // et `0.0 >= 0.0` armerait le verrou dès le premier tour de boucle,
                        // bloquant la filtration pour toute la journée.
                        let objectif_connu =
                            derniere_temperature_eau.is_some() && heures_cibles > 0.0;
                        if objectif_connu && pompe.heures_aujourdhui() >= heures_cibles {
                            filtration_terminee_aujourdhui = true;
                        }
                        // Pas d'objectif fiable (sonde muette) = pas de filtration AUTO,
                        // comme avant l'ajout du verrou : on ne filtre pas « à l'aveugle ».
                        objectif_connu && !filtration_terminee_aujourdhui && dans_plage
                    }
                }
                etat_partage::Mode::Off => false,
            }
        };

        pompe.mettre_a_jour(demande_pompe, systeme_sur);

        // Sauvegarde périodique (pas à chaque tour, l'écriture NVS a un coût et une
        // usure limitée) des compteurs pompe du jour, même cadence que l'horodatage.
        if let Some(date_du_jour) = date_texte.as_deref() {
            if dernier_sauvegarde_pompe_jour.elapsed() >= Duration::from_secs(60) {
                dernier_sauvegarde_pompe_jour = maintenant;
                journal.lock().sauvegarder_pompe_jour(
                    date_du_jour,
                    pompe.heures_aujourdhui(),
                    pompe.sessions_aujourdhui(),
                );
            }
        }

        pcf8574::piloter_pompe(&mut sorties, &i2c_partage, pompe.etat())?;
        pcf8574::piloter_defaut_systeme(&mut sorties, &i2c_partage, !systeme_sur)?;

        // --- Journal : sessions pompe (début/fin) + alertes (front montant de chaque
        // condition), persistés en NVS (historique borné, voir journal.rs). ---
        {
            let date_ref = date_texte.as_deref().unwrap_or("--/--/----");
            let heure_ref = heure_texte.as_deref().unwrap_or("--:--:--");
            let mut jrn = journal.lock();

            let etat_pompe = pompe.etat();
            if etat_pompe != pompe_precedent {
                pompe_transition = Some(etat_pompe);
            }
            if etat_pompe && !pompe_precedent {
                session_debut = Some((heure_ref.to_string(), maintenant));
            }
            if !etat_pompe && pompe_precedent {
                if let Some((debut_str, debut_instant)) = session_debut.take() {
                    let duree_min = maintenant.duration_since(debut_instant).as_secs() as i64 / 60;
                    // Cause approximative (pas de suivi précis du motif comme dans le C++) :
                    // priorité à la sécurité, sinon boost encore actif (force l'arrêt), sinon normal.
                    let cause_fin = if !systeme_sur {
                        "SECURITE"
                    } else if boost.actif() {
                        "BOOST"
                    } else {
                        "NORMAL"
                    };
                    jrn.enregistrer_session(journal::SessionTerminee {
                        date: date_ref,
                        debut: &debut_str,
                        fin: heure_ref,
                        duree_min,
                        t_eau: derniere_temperature_eau.unwrap_or(f32::NAN),
                        mode: mode_texte,
                        cause_fin,
                        pompe_on: true,
                    });
                }
            }
            pompe_precedent = etat_pompe;

            if !niveau_ok && niveau_ok_precedent {
                jrn.enregistrer_alerte(date_ref, heure_ref, "NIVEAU_EAU", "BAS");
                alertes_jour += 1;
            }
            niveau_ok_precedent = niveau_ok;

            if securite_moteur.actif() && !securite_actif_precedent {
                let valeur = if securite_moteur.verrouille() {
                    "VERROUILLE"
                } else {
                    "DEFAUT"
                };
                jrn.enregistrer_alerte(date_ref, heure_ref, "DEFAUT_MOTEUR", valeur);
                alertes_jour += 1;
            }
            securite_actif_precedent = securite_moteur.actif();

            if sonde_muette && !sonde_muette_precedente {
                jrn.enregistrer_alerte(date_ref, heure_ref, "SONDE_EAU_MUETTE", "-");
                alertes_jour += 1;
            }
            sonde_muette_precedente = sonde_muette;

            if pompe.bloquee() && !pompe_bloquee_precedent {
                jrn.enregistrer_alerte(date_ref, heure_ref, "CLAQUEMENT", "-");
                alertes_jour += 1;
            }
            pompe_bloquee_precedent = pompe.bloquee();

            if filtration_auto.anti_gel_actif() && !anti_gel_precedent {
                let valeur = format!("{:.1}", derniere_temperature_eau.unwrap_or(f32::NAN));
                jrn.enregistrer_alerte(date_ref, heure_ref, "ANTI_GEL", &valeur);
                alertes_jour += 1;
            }
            anti_gel_precedent = filtration_auto.anti_gel_actif();

            if filtration_auto.canicule_actif() && !canicule_precedent {
                let valeur = format!("{:.1}", derniere_temperature_eau.unwrap_or(f32::NAN));
                jrn.enregistrer_alerte(date_ref, heure_ref, "CANICULE", &valeur);
                alertes_jour += 1;
            }
            canicule_precedent = filtration_auto.canicule_actif();
        }

        let wifi_connecte = wifi
            .as_ref()
            .is_some_and(|w| w.is_connected().unwrap_or(false));
        let wifi_rssi = wifi.as_ref().and_then(|w| w.wifi().get_rssi().ok());

        if wifi_connecte
            && maintenant.duration_since(dernier_envoi_periodique) >= INTERVALLE_ENVOI_CLOUD
        {
            dernier_envoi_periodique = maintenant;
            if config::ADAFRUIT_IO_ACTIF {
                expediteur_adafruit.envoyer(adafruit_io::Mesures {
                    temp_eau: derniere_temperature_eau,
                    temp_air: derniere_temperature_air,
                    humidite: derniere_humidite,
                    batterie: tension_batterie,
                    sortie_5v: tension_sortie_5v,
                    ..Default::default()
                });
            }
        }

        // État pompe et mode vers Adafruit IO : uniquement à chaque changement (pas
        // toutes les 5 min comme les températures), pour un signal en créneaux net —
        // mais jamais plus vite que `DELAI_MIN_EVENEMENT`. Si le Wi-Fi est
        // indisponible, ou que le délai minimum n'est pas encore écoulé, l'événement
        // reste simplement en attente (`.take()` non consommé) et partira dès que les
        // deux conditions seront réunies, avec la valeur la plus récente si l'état a
        // changé plusieurs fois entre-temps.
        if wifi_connecte
            && (pompe_transition.is_some() || mode_transition.is_some())
            && maintenant.duration_since(dernier_envoi_evenement) >= DELAI_MIN_EVENEMENT
        {
            let nouvel_etat_pompe = pompe_transition.take();
            let nouveau_mode = mode_transition.take().map(|m| match m {
                etat_partage::Mode::Off => 0,
                etat_partage::Mode::Manuel => 1,
                etat_partage::Mode::Auto => 2,
            });
            dernier_envoi_evenement = maintenant;
            if config::ADAFRUIT_IO_ACTIF {
                expediteur_adafruit.envoyer(adafruit_io::Mesures {
                    pompe: nouvel_etat_pompe,
                    mode: nouveau_mode,
                    ..Default::default()
                });
            }
        }

        // Reconnexion Wi-Fi automatique si la connexion a été perdue (coupure routeur,
        // portée...) — auparavant, une perte de Wi-Fi était définitive jusqu'au reboot.
        // Absente si le Wi-Fi est désactivé (`config::WIFI_ACTIF = false`, test temporaire).
        if let Some(w) = wifi.as_mut() {
            if !wifi_connecte && maintenant.duration_since(dernier_controle_wifi) >= INTERVALLE_WIFI
            {
                dernier_controle_wifi = maintenant;
                warn!("Wi-Fi déconnecté, tentative de reconnexion...");
                if let Err(e) = w.wifi_mut().connect() {
                    warn!("Wi-Fi : échec de la demande de reconnexion : {:?}", e);
                }
            }
        }

        // --- Écran OLED : alerte prioritaire, sinon veille après inactivité, sinon page courante ---
        // Le contenu ne se redessine (et ne se flush sur l'I2C) que si quelque chose a
        // réellement changé, ou périodiquement — sinon chaque tick (~20 ms) enverrait tout
        // le framebuffer sur le bus I2C, ce qui bloquerait bien plus que le bouton ne le fait gagner.
        // Toute la logique d'affichage est enveloppée dans une fermeture : une erreur ponctuelle
        // (glitch I2C) est juste signalée et ignorée, elle ne doit plus jamais arrêter le programme.
        let alerte_active = !niveau_ok || securite_moteur.actif() || !systeme_sur;

        if ecran_disponible {
            let resultat_ecran: anyhow::Result<()> = (|| {
                // Une alerte réveille toujours l'écran, quelle que soit la cause de la mise en veille.
                if alerte_active {
                    ecran_en_veille = false;
                    derniere_activite = maintenant;
                } else if !ecran_en_veille && derniere_activite.elapsed() >= VEILLE_APRES {
                    ecran_en_veille = true;
                }

                // Commande physique allumage/extinction : envoyée sur tout changement d'état de
                // veille, quelle qu'en soit la cause (alerte, ou réveil par appui bouton détecté
                // plus haut) — auparavant seule l'alerte envoyait cette commande, donc un réveil
                // par bouton redessinait le contenu en mémoire sans jamais rallumer l'écran.
                if ecran_en_veille != veille_precedente {
                    display
                        .set_display_on(!ecran_en_veille)
                        .map_err(|e| anyhow::anyhow!("Erreur alimentation écran : {:?}", e))?;
                }

                let clignotement_a_change = alerte_active
                    && dernier_clignotement.elapsed() >= Duration::from_millis(600)
                    && {
                        dernier_clignotement = maintenant;
                        alerte_visible = !alerte_visible;
                        true
                    };

                let besoin_redessiner = page_ecran != page_ecran_precedente
                    || alerte_active != alerte_precedente
                    || ecran_en_veille != veille_precedente
                    || clignotement_a_change
                    || (!alerte_active
                        && !ecran_en_veille
                        && dernier_dessin.elapsed() >= INTERVALLE_DESSIN);

                if besoin_redessiner && !ecran_en_veille {
                    if alerte_active {
                        if alerte_visible {
                            let cause_niveau = if !niveau_ok { "Niveau: MANQUE !" } else { "" };
                            let cause_moteur = if securite_moteur.actif() {
                                if securite_moteur.verrouille() {
                                    "Moteur: VERROU !"
                                } else {
                                    "Moteur: DEFAUT !"
                                }
                            } else {
                                ""
                            };
                            ecran::afficher_alerte(&mut display, cause_niveau, cause_moteur)?;
                        } else {
                            ecran::effacer(&mut display)?;
                        }
                    } else {
                        match page_ecran {
                            0 => ecran::afficher_capteurs(
                                &mut display,
                                pompe.etat(),
                                derniere_temperature_air.unwrap_or(f32::NAN),
                                derniere_humidite.unwrap_or(f32::NAN),
                                derniere_temperature_eau.unwrap_or(f32::NAN),
                            )?,
                            1 => ecran::afficher_connecte(&mut display, &ip_texte, wifi_rssi)?,
                            2 => ecran::afficher_pompe(
                                &mut display,
                                pompe.etat(),
                                boost.actif(),
                                mode_texte,
                                pompe.heures_aujourdhui(),
                                heures_cibles,
                                filtration_auto.anti_gel_actif(),
                                filtration_auto.canicule_actif(),
                                plage_debut,
                                plage_fin,
                                pompe.sessions_aujourdhui(),
                            )?,
                            3 => ecran::afficher_securite(
                                &mut display,
                                niveau_ok,
                                securite_moteur.actif(),
                                securite_moteur.verrouille(),
                                systeme_sur,
                                debut_programme.elapsed().as_secs(),
                            )?,
                            _ => ecran::afficher_bilan(
                                &mut display,
                                heure_locale
                                    .map(|h| h.format("%d/%m/%Y").to_string())
                                    .as_deref()
                                    .unwrap_or("--/--/----"),
                                pompe.heures_aujourdhui(),
                                heures_cibles,
                                temp_min_jour,
                                temp_max_jour,
                            )?,
                        }
                    }
                    display
                        .flush()
                        .map_err(|e| anyhow::anyhow!("Erreur flush : {:?}", e))?;
                    dernier_dessin = maintenant;
                }

                Ok(())
            })();

            if let Err(e) = resultat_ecran {
                warn!("Écran OLED : erreur d'affichage (ignorée) : {:?}", e);
            }
        }

        page_ecran_precedente = page_ecran;
        alerte_precedente = alerte_active;
        veille_precedente = ecran_en_veille;

        // Mise à jour de l'état partagé, consultable par le serveur web
        {
            let mut donnees = etat.lock();
            donnees.temperature_air = derniere_temperature_air;
            donnees.humidite = derniere_humidite;
            donnees.temperature_eau = derniere_temperature_eau;
            donnees.sonde_eau_muette = sonde_muette;
            donnees.sonde_eau_age_s = sonde_eau.secondes_depuis_lecture();
            donnees.mode = mode_physique;
            donnees.defaut_moteur = securite_moteur.actif();
            donnees.defaut_moteur_verrouille = securite_moteur.verrouille();
            donnees.niveau_eau_ok = niveau_ok;
            donnees.pompe_active = pompe.etat();
            donnees.pompe_bloquee = pompe.bloquee();
            donnees.pompe_heures_aujourdhui = pompe.heures_aujourdhui();
            donnees.anti_gel = filtration_auto.anti_gel_actif();
            donnees.canicule = filtration_auto.canicule_actif();
            donnees.filt_objectif_heures = heures_cibles;
            donnees.filt_debut_effectif = plage_debut;
            donnees.filt_fin_effective = plage_fin;
            let (config_debut, config_fin) = filtration_auto.plage_configuree();
            donnees.filt_debut_configure = config_debut;
            donnees.filt_fin_configuree = config_fin;
            donnees.boost_actif = boost.actif();
            donnees.boost_restant_secondes = boost.restant_secondes();
            donnees.boost_marche_forcee = boost.marche_forcee();
            donnees.boost_duree_minutes = boost.duree_minutes();
            donnees.wifi_connecte = wifi_connecte;
            donnees.wifi_rssi = wifi_rssi;
            donnees.gps_ok = gps_ok;
            donnees.gps_satellites = gps_satellites;
            donnees.gps_latitude = gps_position.map(|(lat, _)| lat);
            donnees.gps_longitude = gps_position.map(|(_, lon)| lon);
            donnees.heure_automate = heure_automate.clone();
            donnees.historique_modes = historique.segments().to_vec();
            donnees.historique_pompe = historique_pompe.segments().to_vec();
            donnees.tension_batterie_v = tension_batterie;
            donnees.tension_sortie_5v_v = tension_sortie_5v;
        }

        if dernier_log.elapsed() >= INTERVALLE_LOG {
            dernier_log = maintenant;
            // RAM libre (tas) : `libre` = disponible maintenant, `plancher` = le pire
            // niveau jamais atteint depuis le démarrage (utile pour repérer une fuite
            // mémoire lente, ou une carte dont la RAM se dégrade avec le temps).
            // Fonctions ESP-IDF brutes (FFI) : pas d'équivalent sûr côté esp-idf-hal.
            let ram_libre_ko = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() } / 1024;
            let ram_plancher_ko =
                unsafe { esp_idf_svc::sys::esp_get_minimum_free_heap_size() } / 1024;
            info!(
                "Air : {:.1} °C | Humidité : {:.1} % | Eau : {:.1} °C | Niveau : {} | Pompe : {} (sûr:{}, cible:{:.1}h, fait:{:.1}h, boost:{}) | Wi-Fi : {} ({:?} dBm) | GPS : {} ({} sat) | Batterie : {:.2}V | Sortie 5V : {:.2}V | RAM libre : {} Ko (plancher : {} Ko) | Heure : {}",
                derniere_temperature_air.unwrap_or(f32::NAN),
                derniere_humidite.unwrap_or(f32::NAN),
                derniere_temperature_eau.unwrap_or(f32::NAN),
                niveau_ok,
                pompe.etat(),
                systeme_sur,
                heures_cibles,
                pompe.heures_aujourdhui(),
                boost.actif(),
                wifi_connecte,
                wifi_rssi,
                gps_ok,
                gps_satellites,
                tension_batterie.unwrap_or(f32::NAN),
                tension_sortie_5v.unwrap_or(f32::NAN),
                ram_libre_ko,
                ram_plancher_ko,
                heure_automate.as_deref().unwrap_or("indisponible")
            );
        }

        // Court : laisse le temps aux autres tâches FreeRTOS (Wi-Fi, serveur web) de
        // tourner, sans ralentir la lecture du bouton comme le faisait le délai de 2 s.
        FreeRtos::delay_ms(20);
    }
}
