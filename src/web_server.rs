use parking_lot::Mutex;
use std::sync::Arc;

use esp_idf_svc::http::server::{
    Configuration as HttpConfig, EspHttpConnection, EspHttpServer, Request,
};
use esp_idf_svc::io::{EspIOError, Write as EspWrite};
use log::info;

use crate::etat_partage::EtatPartage;
use crate::journal::Journal;

// Deux dashboards possibles, choisis à la compilation via `config::DASHBOARD_SIMPLIFIE` :
// le complet (contrôle pompe, mode, filtration, journaux) ou une version allégée en
// lecture seule (température eau/air, humidité, GPS/position/Wi-Fi) pour un montage
// identique installé ailleurs, dédié à la simple consultation.
const INDEX_HTML: &str = if crate::config::DASHBOARD_SIMPLIFIE {
    include_str!("index_simple.html")
} else {
    include_str!("index.html")
};
const SCRIPT_JS: &str = if crate::config::DASHBOARD_SIMPLIFIE {
    include_str!("script_simple.js")
} else {
    include_str!("script.js")
};

/// Logo Phil Domo recadré en icône (64x64, ~4,6 Ko). Les navigateurs demandent
/// automatiquement `/favicon.ico` (et parfois les icônes Apple) à chaque chargement de
/// page, qu'on le veuille ou non — avant cette route, ces requêtes tombaient sur le
/// traitement générique "404 non trouvé", plus lent, jamais mis en cache, et
/// consommant une des 4 connexions simultanées que le serveur peut gérer. Servir une
/// vraie image ici, avec un cache long, répond plus vite et libère cette connexion
/// pour les ressources qui comptent vraiment (page, script, données).
const FAVICON: &[u8] = include_bytes!("favicon.png");

/// Sert un contenu figé (la page, le script, l'icône) en évitant de le retransmettre
/// quand le navigateur l'a déjà.
///
/// ## Le problème
///
/// La page fait 42 Ko et le script 37 Ko. Ils ne changent **qu'au reflashage**, mais
/// étaient jusqu'ici marqués `max-age=30` : passé trente secondes, le navigateur jetait sa
/// copie et redemandait les 80 Ko. Or ces gros transferts sont précisément ce qui échoue
/// quand la liaison perd des paquets — mesuré le 15/08/2026 : `/sensors` (1,3 Ko) passait
/// sans problème pendant que la page complète était coupée à 20 s.
///
/// ## Le mécanisme retenu — revalidation par `ETag`
///
/// On envoie une étiquette de version (`ETag`) avec le contenu, et `no-cache`, qui ne veut
/// pas dire « ne cache pas » mais « garde une copie, et demande-moi avant de t'en servir ».
///
/// Au chargement suivant, le navigateur renvoie cette étiquette dans `If-None-Match`. Si
/// elle correspond, on répond **304 Non modifié** sans aucun corps : environ 200 octets au
/// lieu de 42 Ko. Sinon on renvoie le contenu complet.
///
/// ## Pourquoi pas simplement un cache long
///
/// Un `max-age` d'un an supprimerait complètement le transfert, mais le navigateur
/// continuerait d'afficher l'ancienne page après un reflashage — exactement l'incident du
/// 02/08/2026, où des données figées ne se débloquaient qu'avec un rechargement forcé. La
/// contourner demanderait un numéro de version dans chaque adresse, à maintenir à la main.
///
/// Ici l'étiquette **est** la version du firmware : elle change toute seule à chaque flash,
/// et le navigateur retélécharge automatiquement. Impossible de servir une page périmée.
fn servir_contenu_fige(
    req: Request<&mut EspHttpConnection<'_>>,
    etag: &str,
    type_contenu: &str,
    corps: &[u8],
) -> Result<(), EspIOError> {
    if req.header("If-None-Match") == Some(etag) {
        req.into_status_response(304)?;
        return Ok(());
    }

    let mut resp = req.into_response(
        200,
        None,
        &[
            ("Content-Type", type_contenu),
            ("Cache-Control", "no-cache"),
            ("ETag", etag),
        ],
    )?;
    resp.write_all(corps)?;
    Ok(())
}

/// Extrait la valeur d'un paramètre de requête simple (`?a=1&b=2`), sans décodage URL
/// (les valeurs utilisées ici — nombres, mots-clés — n'en ont pas besoin).
fn parametre<'a>(uri: &'a str, cle: &str) -> Option<&'a str> {
    let prefixe = format!("{cle}=");
    uri.split('?')
        .nth(1)?
        .split('&')
        .find_map(|kv| kv.strip_prefix(prefixe.as_str()))
}

/// Démarre le serveur web et déclare toutes les routes.
/// `etat` est l'état partagé (température, humidité, mode...) alimenté par la boucle principale.
/// `journal` est l'historique persisté (sessions/bilans/alertes), lu et effacé depuis ces routes,
/// écrit depuis la boucle principale.
pub fn demarrer(
    etat: EtatPartage,
    journal: Arc<Mutex<Journal>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    // `max_open_sockets` : nombre de connexions clients simultanées. La valeur par
    // défaut d'`esp-idf-svc` est 4, nettement plus restrictive que celle d'ESP-IDF
    // lui-même, qui utilise 7. Or un navigateur ouvre jusqu'à 6 connexions en
    // parallèle vers un même serveur : à l'ouverture du dashboard il réclame d'un
    // coup la page (~42 Ko), le script (~37 Ko) et l'icône. Avec deux appareils, les
    // 4 places étaient saturées et l'acceptation des connexions bloquait — mesuré le
    // 08/08/2026 : 7,04 s rien que pour établir la connexion sur `/script.js`, au
    // moment même où `/sensors` se connectait en 6 ms (donc pas un souci radio).
    //
    // Le serveur réserve 3 sockets pour son usage interne (écoute, contrôle,
    // messages) : il faut donc `CONFIG_LWIP_MAX_SOCKETS >= max_open_sockets + 3`,
    // porté à 16 dans `sdkconfig.defaults` pour laisser de la marge à NTP, Adafruit
    // IO et la sonde de `surveillance_reseau.rs`.
    let mut server = EspHttpServer::new(&HttpConfig {
        max_open_sockets: 7,
        ..Default::default()
    })?;

    // Étiquette de version servant d'`ETag` aux contenus figés (page, script, icône).
    // Construite une seule fois au démarrage, pas à chaque requête. Elle change à chaque
    // flash puisqu'elle reprend `VERSION_FIRMWARE` : le navigateur retélécharge donc
    // automatiquement après une mise à jour, sans qu'on ait rien à maintenir.
    // Les guillemets font partie de la syntaxe d'un ETag HTTP.
    let etag = format!("\"{}\"", crate::config::VERSION_FIRMWARE);

    // --- Page principale ---
    // Historique du cache sur cette route : un premier correctif (02/08/2026) avait mis
    // `no-store` partout pour éviter qu'un navigateur ne garde une page périmée après un
    // reflash. Mais forcer la retransmission complète des 42 Ko à chaque chargement a
    // révélé une fragilité du serveur web de l'ESP32 sur les gros envois
    // (`httpd_sock_err: error in send : 11`, page vide côté navigateur). Compromis
    // intermédiaire : `max-age=30`, qui retransmettait quand même tout toutes les 30 s.
    //
    // Depuis le 16/08/2026, revalidation par `ETag` (voir `servir_contenu_fige`) : le
    // navigateur garde sa copie et se contente de demander « a-t-elle changé ? ». Réponse
    // de ~200 octets tant que la version du firmware est la même, contenu complet sinon.
    // On supprime le gros transfert périodique **sans** risquer la page périmée.
    let etag_index = etag.clone();
    server.fn_handler(
        "/",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            servir_contenu_fige(req, &etag_index, "text/html", INDEX_HTML.as_bytes())
        },
    )?;

    // --- Script JavaScript ---
    // Même traitement que la page : 37 Ko qui ne changent qu'au reflash.
    let etag_script = etag.clone();
    server.fn_handler(
        "/script.js",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            servir_contenu_fige(
                req,
                &etag_script,
                "application/javascript",
                SCRIPT_JS.as_bytes(),
            )
        },
    )?;

    // --- Favicon (voir commentaire sur la constante FAVICON) ---
    for chemin in [
        "/favicon.ico",
        "/apple-touch-icon.png",
        "/apple-touch-icon-precomposed.png",
    ] {
        server.fn_handler(
            chemin,
            esp_idf_svc::http::Method::Get,
            |req| -> Result<(), EspIOError> {
                let mut resp = req.into_response(
                    200,
                    None,
                    &[
                        ("Content-Type", "image/png"),
                        ("Cache-Control", "max-age=604800"),
                    ],
                )?;
                resp.write_all(FAVICON)?;
                Ok(())
            },
        )?;
    }

    // --- Données capteurs (JSON) ---
    let etat_sensors = etat.clone();
    server.fn_handler(
        "/sensors",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let donnees = etat_sensors.lock();

            let temp_air = match donnees.temperature_air {
                Some(v) => v.to_string(),
                None => "null".to_string(),
            };
            let hum = match donnees.humidite {
                Some(v) => v.to_string(),
                None => "null".to_string(),
            };
            let temp_eau = match donnees.temperature_eau {
                Some(v) => v.to_string(),
                None => "null".to_string(),
            };
            let mode = match donnees.mode {
                crate::etat_partage::Mode::Auto => "AUTO",
                crate::etat_partage::Mode::Manuel => "MANU",
                crate::etat_partage::Mode::Off => "OFF",
            };
            let wifi_rssi = donnees.wifi_rssi.unwrap_or(0);
            let wifi_ip = donnees.wifi_ip.clone().unwrap_or_default();
            let gps_ok = donnees.gps_ok;
            let gps_sats = donnees.gps_satellites;
            let gps_lat = donnees
                .gps_latitude
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());
            let gps_lon = donnees
                .gps_longitude
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string());
            let heure_automate = donnees.heure_automate.clone().unwrap_or_default();
            let batterie_v = match donnees.tension_batterie_v {
                Some(v) => v.to_string(),
                None => "null".to_string(),
            };
            let sortie_5v_v = match donnees.tension_sortie_5v_v {
                Some(v) => v.to_string(),
                None => "null".to_string(),
            };
            let mode_history = donnees
                .historique_modes
                .iter()
                .map(|seg| {
                    format!(
                        r#"{{"s":{:.3},"e":{:.3},"t":{}}}"#,
                        seg.debut, seg.fin, seg.type_segment
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let pump_history = donnees
                .historique_pompe
                .iter()
                .map(|seg| {
                    format!(
                        r#"{{"s":{:.3},"e":{:.3},"t":{}}}"#,
                        seg.debut, seg.fin, seg.type_segment
                    )
                })
                .collect::<Vec<_>>()
                .join(",");

            let json = format!(
                r#"{{"temperature":{temp_air},"humidity":{hum},"waterTemperature":{temp_eau},"waterProbeStale":{probe_stale},"waterProbeAge":{probe_age},"pumpActive":{pump},"pumpBlocked":{pblocked},"filtFait":{ffait},"filtObjectif":{fobj},"filtDebut":{fdeb},"filtFin":{ffin},"waterLevel":{level},"motorFault":{mfault},"motorFaultLatched":{mlatch},"pcf8574Fault":{pfault},"antiGel":{gel},"canicule":{can},"mode":"{mode}","boostActive":{bactive},"boostRemaining":{brem},"boostForceOn":{bforce},"boostDuration":{bdur},"wifiRssi":{wrssi},"wifiIp":"{wip}","gpsOk":{gok},"gpsSats":{gsat},"gpsLat":{glat},"gpsLon":{glon},"heureAutomate":"{heure}","modeHistory":[{mode_history}],"pumpHistory":[{pump_history}],"batteryV":{batt_v},"solarOutV":{sortie_v}}}"#,
                temp_air = temp_air,
                hum = hum,
                temp_eau = temp_eau,
                probe_stale = donnees.sonde_eau_muette,
                probe_age = donnees
                    .sonde_eau_age_s
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                pump = donnees.pompe_active,
                pblocked = donnees.pompe_bloquee,
                ffait = donnees.pompe_heures_aujourdhui,
                fobj = donnees.filt_objectif_heures,
                fdeb = donnees.filt_debut_effectif,
                ffin = donnees.filt_fin_effective,
                level = donnees.niveau_eau_ok,
                mfault = donnees.defaut_moteur,
                mlatch = donnees.defaut_moteur_verrouille,
                pfault = donnees.pcf8574_injoignable,
                gel = donnees.anti_gel,
                can = donnees.canicule,
                mode = mode,
                bactive = donnees.boost_actif,
                brem = donnees.boost_restant_secondes,
                bforce = donnees.boost_marche_forcee,
                bdur = donnees.boost_duree_minutes,
                wrssi = wifi_rssi,
                wip = wifi_ip,
                gok = gps_ok,
                gsat = gps_sats,
                glat = gps_lat,
                glon = gps_lon,
                heure = heure_automate,
                batt_v = batterie_v,
                sortie_v = sortie_5v_v,
            );
            drop(donnees);

            let mut resp = req.into_response(
                200,
                None,
                &[
                    ("Content-Type", "application/json"),
                    ("Cache-Control", "no-store"),
                ],
            )?;
            resp.write_all(json.as_bytes())?;
            Ok(())
        },
    )?;

    // --- Réarmement manuel du défaut moteur ---
    let etat_reset = etat.clone();
    server.fn_handler(
        "/reset_motor_fault",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            etat_reset.lock().demande_rearmement_moteur = true;
            info!("Demande de réarmement défaut moteur reçue depuis l'interface web");

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"OK")?;
            Ok(())
        },
    )?;

    // --- Commande manuelle de la pompe (boutons MARCHE/ARRÊT, mode MANU uniquement) ---
    let etat_pump = etat.clone();
    server.fn_handler(
        "/pump",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let uri = req.uri().to_string();
            let statut = uri
                .split('?')
                .nth(1)
                .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("status=")));

            let mut donnees = etat_pump.lock();
            if donnees.mode != crate::etat_partage::Mode::Manuel {
                drop(donnees);
                let mut resp = req.into_response(403, None, &[("Content-Type", "text/plain")])?;
                resp.write_all("Commande refusee : passez en mode MANUEL".as_bytes())?;
                return Ok(());
            }

            let demande = match statut {
                Some("on") => true,
                Some("off") => false,
                _ => {
                    drop(donnees);
                    let mut resp =
                        req.into_response(400, None, &[("Content-Type", "text/plain")])?;
                    resp.write_all("Parametre 'status' manquant ou invalide (on/off)".as_bytes())?;
                    return Ok(());
                }
            };
            donnees.demande_pompe_manuelle = demande;
            drop(donnees);
            info!(
                "Commande pompe reçue depuis l'interface web : {}",
                if demande { "MARCHE" } else { "ARRET" }
            );

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"OK")?;
            Ok(())
        },
    )?;

    // --- Marche/arrêt forcé temporaire (boost, mode AUTO uniquement) ---
    let etat_boost = etat.clone();
    server.fn_handler(
        "/boost",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let uri = req.uri().to_string();
            let action = parametre(&uri, "action");

            let mut donnees = etat_boost.lock();
            if donnees.mode != crate::etat_partage::Mode::Auto {
                drop(donnees);
                let mut resp = req.into_response(403, None, &[("Content-Type", "text/plain")])?;
                resp.write_all("Boost disponible uniquement en mode AUTO".as_bytes())?;
                return Ok(());
            }

            match action {
                Some("start") => donnees.demande_boost_start = true,
                Some("stop") => donnees.demande_boost_stop = true,
                _ => {
                    drop(donnees);
                    let mut resp =
                        req.into_response(400, None, &[("Content-Type", "text/plain")])?;
                    resp.write_all(
                        "Parametre 'action' manquant ou invalide (start/stop)".as_bytes(),
                    )?;
                    return Ok(());
                }
            }
            drop(donnees);

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"OK")?;
            Ok(())
        },
    )?;

    // --- Durée du boost (multiple de 30 min, 30 à 480 min) ---
    let etat_boost_duree = etat.clone();
    server.fn_handler(
        "/set-boost-duration",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let uri = req.uri().to_string();
            let minutes = parametre(&uri, "minutes").and_then(|v| v.parse::<u32>().ok());

            let Some(minutes) = minutes else {
                let mut resp = req.into_response(400, None, &[("Content-Type", "text/plain")])?;
                resp.write_all("Parametre 'minutes' manquant ou invalide".as_bytes())?;
                return Ok(());
            };
            etat_boost_duree.lock().demande_boost_duree = Some(minutes);

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"OK")?;
            Ok(())
        },
    )?;

    // --- Lecture de la plage horaire standard configurée ---
    let etat_schedule = etat.clone();
    server.fn_handler(
        "/schedule",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let donnees = etat_schedule.lock();
            let json = format!(
                r#"{{"start":{start},"end":{end}}}"#,
                start = donnees.filt_debut_configure,
                end = donnees.filt_fin_configuree,
            );
            drop(donnees);

            let mut resp = req.into_response(200, None, &[("Content-Type", "application/json")])?;
            resp.write_all(json.as_bytes())?;
            Ok(())
        },
    )?;

    // --- Modification de la plage horaire standard ---
    let etat_set_schedule = etat.clone();
    server.fn_handler(
        "/set-schedule",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let uri = req.uri().to_string();
            let debut = parametre(&uri, "start").and_then(|v| v.parse::<f32>().ok());
            let fin = parametre(&uri, "end").and_then(|v| v.parse::<f32>().ok());

            let (Some(debut), Some(fin)) = (debut, fin) else {
                let mut resp = req.into_response(400, None, &[("Content-Type", "text/plain")])?;
                resp.write_all("Parametres manquants (start, end)".as_bytes())?;
                return Ok(());
            };
            if !(0.0..=23.5).contains(&debut) || !(0.5..=24.0).contains(&fin) || debut >= fin {
                let mut resp = req.into_response(400, None, &[("Content-Type", "text/plain")])?;
                resp.write_all(
                    "Valeurs invalides (0<=debut<=23.5, 0.5<=fin<=24, debut<fin)".as_bytes(),
                )?;
                return Ok(());
            }
            etat_set_schedule.lock().demande_plage = Some((debut, fin));

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"OK")?;
            Ok(())
        },
    )?;

    // --- Journaux (sessions / bilans journaliers / alertes) ---
    let journal_sessions = journal.clone();
    server.fn_handler(
        "/log/sessions",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let texte = journal_sessions.lock().lire_sessions();
            let mut resp = req.into_response(200, None, &[("Content-Type", "text/plain")])?;
            resp.write_all(texte.as_bytes())?;
            Ok(())
        },
    )?;

    let journal_daily = journal.clone();
    server.fn_handler(
        "/log/daily",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let texte = journal_daily.lock().lire_bilans();
            let mut resp = req.into_response(200, None, &[("Content-Type", "text/plain")])?;
            resp.write_all(texte.as_bytes())?;
            Ok(())
        },
    )?;

    let journal_alertes = journal.clone();
    server.fn_handler(
        "/log/alertes",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            let texte = journal_alertes.lock().lire_alertes();
            let mut resp = req.into_response(200, None, &[("Content-Type", "text/plain")])?;
            resp.write_all(texte.as_bytes())?;
            Ok(())
        },
    )?;

    let journal_clear = journal.clone();
    server.fn_handler(
        "/clear-logs",
        esp_idf_svc::http::Method::Get,
        move |req| -> Result<(), EspIOError> {
            journal_clear.lock().effacer_tout();
            info!("Journaux effacés depuis l'interface web");

            let mut resp = req.into_ok_response()?;
            resp.write_all(b"OK")?;
            Ok(())
        },
    )?;

    info!("Serveur web démarré");

    Ok(server)
}
