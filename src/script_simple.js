/**
 * DASHBOARD SIMPLIFIÉ — lecture seule (température eau/air, humidité).
 * Réutilise la même route /sensors que le dashboard complet, mais n'affiche
 * qu'un sous-ensemble des champs — voir config::DASHBOARD_SIMPLIFIE.
 */

// ============================================================
// 1. HORLOGE
// ============================================================

function updateDateTime() {
  const now = new Date();
  const optionsDate = { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' };

  const dateEl = document.querySelector('.date');
  if (dateEl) {
    dateEl.textContent = now.toLocaleDateString('fr-FR', optionsDate);
  }

  const h = String(now.getHours()).padStart(2, '0');
  const m = String(now.getMinutes()).padStart(2, '0');
  const s = String(now.getSeconds()).padStart(2, '0');
  const timeEl = document.querySelector('.time');
  if (timeEl) {
    timeEl.innerHTML = `${h}:${m}<span class="blink">:</span>${s}`;
  }
}

function setTextContent(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
}

// ============================================================
// 2. VIGNETTES GPS / POSITION / WIFI
// ============================================================

function updateGPS(data) {
  const badge = document.getElementById('gpsBadge');
  const val   = document.getElementById('gpsValue');
  if (!badge || !val) return;

  const sats = data.gpsSats != null ? data.gpsSats : 0;
  const ok   = data.gpsOk   === true;

  updatePosition(data);

  badge.classList.remove('gps-ok', 'gps-weak', 'gps-none');

  if (ok) {
    badge.classList.add('gps-ok');
    val.style.color = '#10b981';
    val.textContent = sats + ' SAT';
  } else if (sats > 0) {
    badge.classList.add('gps-weak');
    val.style.color = '#f59e0b';
    val.textContent = sats + ' SAT';
  } else {
    badge.classList.add('gps-none');
    val.style.color = '#ef4444';
    val.textContent = 'Signal perdu';
  }
}

function formaterCoordonnee(valeur, lettrePositive, lettreNegative) {
  const lettre = valeur >= 0 ? lettrePositive : lettreNegative;
  return Math.abs(valeur).toFixed(5) + '° ' + lettre;
}

function updatePosition(data) {
  const badge = document.getElementById('positionBadge');
  const val   = document.getElementById('positionValue');
  if (!badge || !val) return;

  if (typeof data.gpsLat !== 'number' || typeof data.gpsLon !== 'number') {
    badge.style.display = 'none';
    return;
  }

  const lat = data.gpsLat, lon = data.gpsLon;
  badge.style.display = '';
  badge.classList.add('position-ok');
  val.textContent = formaterCoordonnee(lat, 'N', 'S') + ', ' + formaterCoordonnee(lon, 'E', 'O');
  badge.href = 'https://www.openstreetmap.org/?mlat=' + lat + '&mlon=' + lon + '#map=17/' + lat + '/' + lon;
}

function updateWifi(data) {
  const badge = document.getElementById('wifiBadge');
  const val   = document.getElementById('wifiValue');
  if (!badge || !val) return;

  const rssi = data.wifiRssi != null ? data.wifiRssi : 0;

  badge.classList.remove('wifi-ok', 'wifi-weak', 'wifi-none');

  if (rssi === 0) {
    badge.classList.add('wifi-none');
    val.style.color = '#ef4444';
    val.textContent = 'Hors ligne';
  } else if (rssi < -80) {
    badge.classList.add('wifi-none');
    val.style.color = '#ef4444';
    val.textContent = rssi + ' dBm';
  } else if (rssi < -65) {
    badge.classList.add('wifi-weak');
    val.style.color = '#f59e0b';
    val.textContent = rssi + ' dBm';
  } else {
    badge.classList.add('wifi-ok');
    val.style.color = '#10b981';
    val.textContent = rssi + ' dBm';
  }
}

function updateTimeSource(data) {
  const el = document.getElementById('timeSource');
  if (!el) return;
  el.className = 'time-source';
  const heure = data.heureAutomate ? (' · ' + data.heureAutomate) : '';
  if (data.gpsOk) {
    el.classList.add('time-source-gps');
    el.textContent = '🛰 GPS' + heure;
  } else if (data.wifiRssi && data.wifiRssi !== 0) {
    el.classList.add('time-source-ntp');
    el.textContent = '⇅ NTP' + heure;
  } else {
    el.classList.add('time-source-none');
    el.textContent = '⚠ FALLBACK';
  }
}

// ============================================================
// 3. RÉCUPÉRATION DES DONNÉES
// ============================================================

let isFetching = false;

function fetchAllData() {
  if (isFetching) return;
  isFetching = true;

  fetch('/sensors')
    .then(response => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return response.json();
    })
    .then(data => {
      if (data.error) {
        setTextContent('temp', '--');
        setTextContent('hum', '--');
      } else {
        setTextContent('temp', data.temperature != null ? data.temperature.toFixed(1) : '--');
        setTextContent('hum',  data.humidity    != null ? data.humidity.toFixed(0)    : '--');
      }

      if (data.waterTemperature != null) {
        setTextContent('tempWater', data.waterTemperature < -50
          ? '---'
          : data.waterTemperature.toFixed(1));
      } else {
        setTextContent('tempWater', '--');
      }

      updateGPS(data);
      updateWifi(data);
      updateTimeSource(data);
    })
    .catch(err => {
      console.error('Erreur récupération données :', err);
    })
    .finally(() => {
      isFetching = false;
    });
}

// ============================================================
// 4. LANCEMENT DES TÂCHES CADENCÉES
// ============================================================

updateDateTime();
fetchAllData();

setInterval(updateDateTime, 1000);
setInterval(fetchAllData, 2000);
