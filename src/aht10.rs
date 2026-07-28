use core::cell::RefCell;
use esp_idf_svc::hal::i2c::I2cDriver;

use crate::config::AHT10_ADDR;

/// Lit la température (°C) et l'humidité (%) sur le capteur AHT10.
pub fn lire_temperature_humidite(i2c: &RefCell<I2cDriver>) -> anyhow::Result<(f32, f32)> {
    i2c.borrow_mut()
        .write(AHT10_ADDR, &[0xAC, 0x33, 0x00], 100)
        .map_err(|e| anyhow::anyhow!("Erreur envoi commande AHT10 : {:?}", e))?;

    // Le capteur a besoin d'environ 75ms pour effectuer la mesure
    esp_idf_svc::hal::delay::FreeRtos::delay_ms(80);

    let mut buffer = [0u8; 6];
    i2c.borrow_mut()
        .read(AHT10_ADDR, &mut buffer, 100)
        .map_err(|e| anyhow::anyhow!("Erreur lecture AHT10 : {:?}", e))?;

    let humidite_brute: u32 =
        ((buffer[1] as u32) << 12) | ((buffer[2] as u32) << 4) | ((buffer[3] as u32) >> 4);
    let humidite = (humidite_brute as f32 / 1048576.0) * 100.0;

    let temperature_brute: u32 =
        (((buffer[3] as u32) & 0x0F) << 16) | ((buffer[4] as u32) << 8) | (buffer[5] as u32);
    let temperature = (temperature_brute as f32 / 1048576.0) * 200.0 - 50.0;

    Ok((temperature, humidite))
}
