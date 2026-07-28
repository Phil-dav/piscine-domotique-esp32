use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::info;

use crate::config::{WIFI_PASSWORD, WIFI_SSID};

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
