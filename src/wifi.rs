use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{esp, esp_wifi_set_ps, wifi_ps_type_t_WIFI_PS_NONE};
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::{info, warn};

use crate::config::{WIFI_PASSWORD, WIFI_SSID};

/// Désactive la veille modem du Wi-Fi.
///
/// ESP-IDF active par défaut `WIFI_PS_MIN_MODEM` en mode station : la puce radio
/// s'endort entre deux balises de la box et ne se réveille que périodiquement.
/// Les paquets qui arrivent pendant le sommeil sont mis en attente par la box,
/// puis délivrés au réveil — un mécanisme fragile dès que le signal n'est pas
/// excellent. Ici le signal tourne autour de -65 à -70 dBm, et cette remise en
/// attente se perd régulièrement : la carte ne répond alors qu'après plusieurs
/// retransmissions.
///
/// Mesuré depuis le PC le 08/08/2026, carte allumée depuis ~7 h :
/// première requête 7,09 s, puis 0,09 s et 0,09 s une fois la puce tenue
/// éveillée par le trafic. C'est exactement la « lenteur qui s'aggrave » quand
/// le dashboard n'a pas été consulté depuis un moment.
///
/// Contrepartie : consommation moyenne plus élevée, la radio ne dort plus.
/// Acceptable ici, la carte est alimentée en permanence (à reconsidérer si le
/// projet passe un jour sur batterie/solaire).
///
/// À réappliquer après chaque `stop()`/`start()` du pilote : on ne suppose pas
/// que le réglage survive à un cycle complet.
pub fn desactiver_economie_energie() {
    match esp!(unsafe { esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE) }) {
        Ok(()) => info!("Wi-Fi : veille modem désactivée (WIFI_PS_NONE)"),
        Err(e) => warn!("Wi-Fi : impossible de désactiver la veille modem : {e:?}"),
    }
}

/// Force une reconnexion en repartant de zéro : `disconnect()` **puis** `connect()`.
///
/// # Pourquoi la déconnexion préalable est indispensable
///
/// Un `connect()` seul ne fait rien du tout quand le pilote se croit déjà associé. Il
/// écrit un avertissement dans son propre journal et **renvoie un succès** :
///
/// ```text
/// W wifi:sta is connected, disconnect before connecting to new ap
/// ```
///
/// Or c'est précisément la situation dans laquelle on se trouve quand on cherche à
/// reconnecter. La capture du 16/08/2026 est sans appel : **44 tentatives, 0 exécutée.**
/// Trente-six ont renvoyé un succès sans rien faire, huit ont échoué en
/// `ESP_ERR_WIFI_CONN`. Le mécanisme de récupération tapait dans le vide depuis sa
/// création.
///
/// L'espion Wi-Fi (`c:\rust\5`) a montré l'autre versant de la scène au même instant :
/// la box émettait des déauthentifications de **raison 7** — « trame reçue d'une station
/// non associée ». Elle avait donc déjà retiré la carte de sa table, pendant que la carte
/// se croyait toujours associée et continuait d'émettre dans le vide. Aucune des deux ne
/// pouvait sortir seule de ce désaccord, et rien au-dessus ne fonctionnait : ni ARP, ni
/// ping, ni DNS.
///
/// `disconnect()` oblige la carte à abandonner cette croyance périmée et à tout
/// reconstruire depuis l'authentification. C'est exactement l'opération que seul
/// `esp_restart()` accomplissait jusqu'ici — d'où le fait qu'il ait toujours été le seul
/// remède efficace.
///
/// L'échec du `disconnect()` n'est pas traité comme une erreur bloquante : s'il échoue
/// parce que la carte n'était effectivement pas associée, le `connect()` qui suit est
/// justement ce qu'il faut faire.
pub fn forcer_reconnexion(wifi: &mut BlockingWifi<EspWifi<'static>>) {
    if let Err(e) = wifi.wifi_mut().disconnect() {
        // Cas normal quand l'association était réellement perdue : on continue.
        info!("Wi-Fi : déconnexion préalable sans effet ({e:?}), poursuite.");
    }
    if let Err(e) = wifi.wifi_mut().connect() {
        warn!("Wi-Fi : échec de la demande de reconnexion : {e:?}");
    }
}

/// Connecte l'ESP32 au réseau Wi-Fi configuré dans config.rs.
/// Scanne d'abord le réseau pour détecter automatiquement son canal
/// et son mode d'authentification réel, plutôt que de les deviner.
pub fn connecter(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), Some(nvs))?, sys_loop)?;

    // Démarrage minimal pour pouvoir scanner
    wifi.set_configuration(&Configuration::Client(ClientConfiguration::default()))?;
    wifi.start()?;
    desactiver_economie_energie();
    info!("Wi-Fi démarré, recherche du réseau {}...", WIFI_SSID);

    let reseaux = wifi.scan()?;
    let reseau_trouve = reseaux.into_iter().find(|r| r.ssid.as_str() == WIFI_SSID);

    let config = match reseau_trouve {
        Some(r) => {
            info!(
                "Réseau trouvé : canal {}, sécurité détectée : {:?}",
                r.channel, r.auth_method
            );
            ClientConfiguration {
                ssid: WIFI_SSID
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("SSID trop long"))?,
                password: WIFI_PASSWORD
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Mot de passe trop long"))?,
                channel: Some(r.channel),
                auth_method: r.auth_method.unwrap_or(AuthMethod::WPA2Personal),
                ..Default::default()
            }
        }
        None => {
            info!("Réseau non trouvé lors du scan, tentative avec les réglages par défaut");
            ClientConfiguration {
                ssid: WIFI_SSID
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("SSID trop long"))?,
                password: WIFI_PASSWORD
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Mot de passe trop long"))?,
                auth_method: AuthMethod::WPA2Personal,
                ..Default::default()
            }
        }
    };

    wifi.set_configuration(&Configuration::Client(config))?;

    wifi.connect()?;
    info!("Wi-Fi connecté");

    wifi.wait_netif_up()?;
    info!("Interface réseau active");

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    info!("Adresse IP obtenue : {:?}", ip_info.ip);

    Ok(wifi)
}
