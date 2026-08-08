/**
 * GESTION DE L'INTERFACE WEB - PISCINE
 * Rôle : Mise à jour de l'affichage, envoi des commandes et détection des défauts.
 */


// ============================================================
// 1. GESTION DE L'HORLOGE
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


// ============================================================
// 2. RÉCUPÉRATION UNIFIÉE DES DONNÉES
// ============================================================

let isFetching = false;
let consecutiveErrors = 0;
const MAX_ERRORS = 3;

function fetchAllData() {
  if (isFetching) return;
  isFetching = true;

  const controleurTimeout = new AbortController();
  const delaiTimeout = setTimeout(() => controleurTimeout.abort(), 8000);

  fetch('/sensors', { signal: controleurTimeout.signal })
    .then(response => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return response.json();
    })
    .then(data => {
      consecutiveErrors = 0;
      hideConnectionError();

      const faults = [];

      if (data.error) {
        // Capteur AHT10 indisponible — température et humidité non disponibles
        faults.push('Capteur ambiance AHT10 indisponible');
        setTextContent('temp', '--');
        setTextContent('hum', '--');
      } else {
        setTextContent('temp',  data.temperature      != null ? data.temperature.toFixed(1)      : '--');
        setTextContent('hum',   data.humidity         != null ? data.humidity.toFixed(0)          : '--');
      }

      // Température eau
      if (data.waterTemperature != null) {
        if (data.waterTemperature < -50) {
          setTextContent('tempWater', '---');
          faults.push(`Sonde DS18B20 déconnectée (valeur lue : ${data.waterTemperature.toFixed(1)}°C)`);
        } else {
          setTextContent('tempWater', data.waterTemperature.toFixed(1));
        }
      } else {
        setTextContent('tempWater', '--');
      }

      // Statut pompe
      const pumpEl  = document.getElementById('pumpStatus');
      const pumpDot = document.getElementById('pumpDot');
      if (pumpEl && data.pumpActive !== undefined) {
        const running = data.pumpActive;
        pumpEl.textContent = running ? 'MARCHE' : 'ARRÊT';
        pumpEl.style.color  = running ? '#00ff88' : '#ff4444';
        if (pumpDot) {
          pumpDot.style.background  = running ? '#00ff88' : '#4a6888';
          pumpDot.style.boxShadow   = running ? '0 0 8px #00ff88' : '0 0 6px #4a6888';
          running ? pumpDot.classList.add('running') : pumpDot.classList.remove('running');
        }
      }

      // Niveau d'eau
      const levelEl  = document.getElementById('waterLevelStatus');
      const cardLevel = document.getElementById('cardLevel');
      if (levelEl && data.waterLevel !== undefined) {
        const ok = data.waterLevel;
        levelEl.textContent = ok ? 'CORRECT' : 'BAS !';
        levelEl.style.color  = ok ? '#00ff88' : '#ff4444';
        if (cardLevel) {
          cardLevel.style.borderColor = ok ? '#00aa44' : '#ff3333';
          cardLevel.style.boxShadow   = ok
            ? '0 4px 16px rgba(0,170,68,0.25)'
            : '0 4px 20px rgba(255,51,51,0.4)';
        }
        if (!ok) faults.push('Niveau d\'eau insuffisant — pompe protégée');
      }

      // Défaut moteur (contact disjoncteur NF ouvert) avec verrou de réarmement
      const motorDot      = document.getElementById('motorFaultDot');
      const motorStatusEl = document.getElementById('motorFaultStatus');
      const btnReset      = document.getElementById('btnResetMotor');
      if (motorStatusEl) {
        if (data.motorFault) {
          const latched = data.motorFaultLatched;
          motorStatusEl.textContent = latched ? 'VERROUILLÉ' : 'DÉFAUT';
          motorStatusEl.style.color = '#ef4444';
          if (motorDot) { motorDot.style.background = '#ef4444'; motorDot.style.boxShadow = '0 0 6px #ef4444'; }
          if (btnReset)  btnReset.style.display = 'block';
          const msg = latched
            ? '⚠ DÉFAUT MOTEUR VERROUILLÉ — réarmement requis (disjoncteur refermé ?)'
            : '⚠ DÉFAUT MOTEUR — disjoncteur déclenché ou câble coupé (J3 PIN7)';
          faults.push(msg);
        } else {
          motorStatusEl.textContent = 'OK';
          motorStatusEl.style.color = '#10b981';
          if (motorDot) { motorDot.style.background = '#10b981'; motorDot.style.boxShadow = '0 0 6px #10b981'; }
          if (btnReset)  btnReset.style.display = 'none';
        }
      }

      if (data.pcf8574Fault) {
        faults.push('⚠ PCF8574 INJOIGNABLE — pompe et relais défaut non pilotés, nouvel essai en continu');
      }

      // Alertes thermiques
      if (data.antiGel)       faults.push('⚠ ANTI-GEL actif — pompe forcée en continu (T° eau < 4°C)');
      if (data.canicule)      faults.push('⚠ CANICULE active — filtration continue (T° eau > 28°C)');
      if (data.pumpBlocked)   faults.push('⛔ POMPE BLOQUÉE — claquement détecté — reprise automatique dans 5 min');
      if (data.waterProbeStale) {
        const age = (typeof data.waterProbeAge === 'number')
          ? ` — dernière mesure il y a ${Math.floor(data.waterProbeAge / 60)} min`
          : ' — aucune mesure depuis le démarrage';
        faults.push(`⚠ SONDE T° EAU MUETTE${age} — valeur affichée figée, relance automatique en cours`);
      }

      // Mode de fonctionnement
      const modeEl   = document.getElementById('currentModeDisplay');
      const cardMode = document.getElementById('cardMode');
      if (modeEl && data.mode) {
        modeEl.textContent = data.mode;
        // Couleur de la valeur
        if (data.mode === 'AUTO') {
          modeEl.style.color = '#00ff88';
        } else if (data.mode === 'MANU') {
          modeEl.style.color = '#ffcc00';
        } else {
          modeEl.style.color = '#888';
        }
        // Classe CSS sur la carte (bordure colorée)
        if (cardMode) {
          cardMode.classList.remove('mode-auto', 'mode-manu', 'mode-off');
          if      (data.mode === 'AUTO') cardMode.classList.add('mode-auto');
          else if (data.mode === 'MANU') cardMode.classList.add('mode-manu');
          else                           cardMode.classList.add('mode-off');
        }
      }

      // Visibilité boutons pompe selon le mode.
      // En MANU, c'est toujours l'utilisateur qui décide (le seul cas où la pompe est
      // forcée quel que soit le mode, c'est le gel absolu < 2°C, géré côté firmware sans
      // passer par ces boutons) — l'anti-gel/canicule ne doit donc masquer le boost qu'en AUTO.
      const pumpButtons  = document.getElementById('pumpButtons');
      const boostButtons = document.getElementById('boostButtons');
      const boostDurSection = document.getElementById('boostDurSection');
      if (data.mode === 'MANU') {
        if (pumpButtons)     pumpButtons.style.display     = 'flex';
        if (boostButtons)    boostButtons.style.display    = 'none';
        if (boostDurSection) boostDurSection.style.display = 'none';
      } else if (data.mode === 'AUTO') {
        if (pumpButtons)     pumpButtons.style.display     = 'none';
        if (data.antiGel || data.canicule) {
          // Protection automatique déjà active : le boost n'a pas à la contredire.
          if (boostButtons)    boostButtons.style.display    = 'none';
          if (boostDurSection) boostDurSection.style.display = 'none';
        } else {
          if (boostButtons)    boostButtons.style.display    = 'flex';
          if (boostDurSection) boostDurSection.style.display = '';
        }
      } else {
        if (pumpButtons)     pumpButtons.style.display     = 'none';
        if (boostButtons)    boostButtons.style.display    = 'none';
        if (boostDurSection) boostDurSection.style.display = 'none';
      }

      // Boutons boost dynamiques selon état pompe + boost actif/inactif
      const btnBoostAction = document.getElementById('btnBoostAction');
      const btnStopBoost   = document.getElementById('btnStopBoost');
      const boostInfo      = document.getElementById('boostInfo');
      const durMin         = data.boostDuration != null ? data.boostDuration : 60;
      const durLabel       = fmtBoostDur(durMin);

      if (data.boostActive && data.boostRemaining > 0) {
        // Boost en cours : afficher Annuler + décompte
        if (btnBoostAction) btnBoostAction.style.display = 'none';
        if (btnStopBoost) {
          btnStopBoost.textContent = data.boostForceOn ? '✕ Annuler marche forcée' : '✕ Annuler arrêt forcé';
          btnStopBoost.style.display = '';
        }
        const rem = data.boostRemaining;
        const bh = Math.floor(rem / 3600);
        const bm = Math.floor((rem % 3600) / 60);
        const bs = rem % 60;
        const txt = (bh > 0 ? bh + 'h ' : '') + String(bm).padStart(2, '0') + 'mn ' + String(bs).padStart(2, '0') + 's';
        const dir = data.boostForceOn ? '⚡ Marche forcée' : '⏹ Arrêt forcé';
        setTextContent('boostCountdown', dir + ' — ' + txt);
        if (boostInfo) boostInfo.style.display = 'block';
      } else {
        // Pas de boost : bouton adapté à l'état actuel de la pompe
        if (btnStopBoost) btnStopBoost.style.display = 'none';
        setTextContent('boostCountdown', '');
        if (boostInfo) boostInfo.style.display = 'none';
        if (btnBoostAction) {
          if (data.pumpActive) {
            btnBoostAction.textContent = '⏹ Arrêt forcé ' + durLabel;
            btnBoostAction.className   = 'btn btn-stop-boost';
          } else {
            btnBoostAction.textContent = '⚡ Marche forcée ' + durLabel;
            btnBoostAction.className   = 'btn btn-boost';
          }
          btnBoostAction.style.display = '';
        }
      }

      updateFiltration(data);
      updateFaults(faults);
      updateGPS(data);
      updateWifi(data);
      updateBattery(data);
      updateTimeSource(data);
    })
    .catch(err => {
      consecutiveErrors++;
      console.error(`Erreur récupération données (${consecutiveErrors}/${MAX_ERRORS}) :`, err);
      if (consecutiveErrors >= MAX_ERRORS) {
        showConnectionError();
        updateFaults(['Connexion perdue avec l\'ESP32 — nouvelle tentative en cours…']);
      }
    })
    .finally(() => {
      clearTimeout(delaiTimeout);
      isFetching = false;
    });
}


// ============================================================
// 3. ENVOI DES COMMANDES (Boutons pompe)
// ============================================================

// ============================================================
// JOURNAUX — Affichage des fichiers CSV
// ============================================================

const _logRoutes = { sessions: '/log/sessions', daily: '/log/daily', alertes: '/log/alertes' };
const _logBtnIds = { sessions: 'btnLogSessions', daily: 'btnLogDaily', alertes: 'btnLogAlertes' };

/**
 * Formate un texte CSV brut en tableau aligné lisible (police monospace).
 * Ajoute une ligne de séparation sous l'en-tête.
 */
function formatCSV(text) {
  const lines = text.trim().split('\n').filter(l => l.trim());
  if (!lines.length) return text;
  const rows = lines.map(l => l.split(',').map(c => c.trim()));
  const maxCols = Math.max(...rows.map(r => r.length));
  const widths  = Array(maxCols).fill(0);
  rows.forEach(row => {
    row.forEach((cell, i) => { if ((cell || '').length > widths[i]) widths[i] = cell.length; });
  });
  const sep = widths.map(w => '─'.repeat(w)).join('─┼─');
  return rows.map((row, ri) => {
    const line = row.map((c, i) => (c || '').padEnd(widths[i])).join(' │ ');
    return ri === 0 ? line + '\n' + sep : line;
  }).join('\n');
}

function closeLog() {
  const display = document.getElementById('logDisplay');
  if (display) {
    display.textContent = 'Sélectionnez un journal ci-dessus.';
    display.style.display = 'none';
  }
  Object.values(_logBtnIds).forEach(id => {
    const b = document.getElementById(id);
    if (b) b.classList.remove('active');
  });
  const closeBtn = document.getElementById('btnLogClose');
  if (closeBtn) closeBtn.style.display = 'none';
}

function showLog(type) {
  // Mise en évidence du bouton actif
  Object.values(_logBtnIds).forEach(id => {
    const b = document.getElementById(id);
    if (b) b.classList.remove('active');
  });
  const activeBtn = document.getElementById(_logBtnIds[type]);
  if (activeBtn) activeBtn.classList.add('active');

  const display = document.getElementById('logDisplay');
  if (!display) return;
  display.style.display = '';
  display.textContent = 'Chargement…';

  const closeBtn = document.getElementById('btnLogClose');
  if (closeBtn) closeBtn.style.display = '';

  fetch(_logRoutes[type])
    .then(r => r.text())
    .then(txt => {
      if (!txt || txt.startsWith('(')) {
        display.textContent = txt || '(vide)';
      } else {
        display.textContent = formatCSV(txt);
      }
    })
    .catch(() => { display.textContent = 'Erreur de lecture du journal.'; });
}

function resetMotorFault() {
  fetch('/reset_motor_fault')
    .then(response => {
      if (!response.ok) return response.text().then(t => { throw new Error(t); });
      console.log('Défaut moteur réarmé');
    })
    .catch(err => {
      alert('Réarmement refusé — ' + err.message);
    });
}

function boostPump() {
  fetch('/boost?action=start')
    .then(response => {
      if (!response.ok) return response.text().then(t => { throw new Error(t); });
    })
    .catch(err => {
      alert('Boost refusé — ' + err.message);
    });
}

function stopBoost() {
  fetch('/boost?action=stop')
    .then(response => {
      if (!response.ok) return response.text().then(t => { throw new Error(t); });
    })
    .catch(err => {
      alert('Erreur — ' + err.message);
    });
}

function controlPump(action) {
  console.log('Action utilisateur : Pompe -> ' + action);

  fetch('/pump?status=' + action)
    .then(response => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      // Pas de mise à jour immédiate : l'état réel sera rafraîchi par fetchAllData()
    })
    .catch(err => {
      console.error("Erreur d'envoi commande pompe :", err);
      alert("Commande refusée — vérifiez que l'interrupteur physique est en mode MANUEL.");
    });
}


// ============================================================
// 4. VIGNETTE GPS
// ============================================================

function updateGPS(data) {
  const badge  = document.getElementById('gpsBadge');
  const val    = document.getElementById('gpsValue');
  if (!badge || !val) return;

  const sats  = data.gpsSats != null ? data.gpsSats : 0;
  const ok    = data.gpsOk   === true;

  updatePosition(data);

  badge.classList.remove('gps-ok', 'gps-weak', 'gps-none');

  if (ok) {
    badge.classList.add('gps-ok');
    badge.style.color = '#10b981';
    val.style.color   = '#10b981';
    val.textContent   = sats + ' SAT';
  } else if (sats > 0) {
    badge.classList.add('gps-weak');
    badge.style.color = '#f59e0b';
    val.style.color   = '#f59e0b';
    val.textContent   = sats + ' SAT';
  } else {
    badge.classList.add('gps-none');
    badge.style.color = '#ef4444';
    val.style.color   = '#ef4444';
    val.textContent   = 'Signal perdu';
  }
}

/**
 * Formate un degré décimal signé en notation géographique classique
 * (valeur absolue + point cardinal), ex. -4.978 -> "4.97800° O".
 */
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


// ============================================================
// 4b. VIGNETTE WiFi
// ============================================================

function updateWifi(data) {
  const badge = document.getElementById('wifiBadge');
  const val   = document.getElementById('wifiValue');
  if (!badge || !val) return;

  const rssi = data.wifiRssi != null ? data.wifiRssi : 0;

  badge.classList.remove('wifi-ok', 'wifi-weak', 'wifi-none');

  if (rssi === 0) {
    badge.classList.add('wifi-none');
    badge.style.color = '#ef4444';
    val.style.color   = '#ef4444';
    val.textContent   = 'Hors ligne';
  } else if (rssi < -80) {
    badge.classList.add('wifi-none');
    badge.style.color = '#ef4444';
    val.style.color   = '#ef4444';
    val.textContent   = rssi + ' dBm';
  } else if (rssi < -65) {
    badge.classList.add('wifi-weak');
    badge.style.color = '#f59e0b';
    val.style.color   = '#f59e0b';
    val.textContent   = rssi + ' dBm';
  } else {
    badge.classList.add('wifi-ok');
    badge.style.color = '#10b981';
    val.style.color   = '#10b981';
    val.textContent   = rssi + ' dBm';
  }
}

// ============================================================
// 4c. VIGNETTE BATTERIE
// ============================================================

function updateBattery(data) {
  const badge = document.getElementById('batteryBadge');
  const val   = document.getElementById('batteryValue');
  const sub   = document.getElementById('batterySubValue');
  if (!badge || !val || !sub) return;

  const v5 = typeof data.solarOutV === 'number' ? data.solarOutV.toFixed(2) + ' V (5V)' : '';
  sub.textContent = v5;

  badge.classList.remove('battery-ok', 'battery-weak', 'battery-none');

  if (typeof data.batteryV !== 'number') {
    badge.classList.add('battery-none');
    badge.style.color = '#ef4444';
    val.style.color   = '#ef4444';
    val.textContent   = '--';
    return;
  }

  const v = data.batteryV;
  let cls, color;
  // Seuils indicatifs pour une batterie plomb/gel 12V (à ajuster une fois des
  // mesures réelles disponibles) : ~12,6V pleine charge au repos, ~11V vide.
  if (v >= 12.0) {
    cls = 'battery-ok'; color = '#10b981';
  } else if (v >= 11.0) {
    cls = 'battery-weak'; color = '#f59e0b';
  } else {
    cls = 'battery-none'; color = '#ef4444';
  }

  badge.classList.add(cls);
  badge.style.color = color;
  val.style.color   = color;
  val.textContent   = v.toFixed(2) + ' V';
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
// 5. VISUEL FILTRATION AUTOMATIQUE
// ============================================================

// Couleurs des segments selon le type : 0=OFF, 1=AUTO, 2=MANU_ON, 3=MANU_OFF, 4=BOOST_ON, 5=BOOST_OFF
const MODE_SEG_COLORS = ['#6b7280', '#3b82f6', '#10b981', '#f59e0b', '#a855f7', '#ef4444'];
const MODE_SEG_LABELS = ['OFF', 'AUTO', 'Manuel ON', 'Manuel OFF', 'Boost ON', 'Boost OFF'];

// Heure décimale (ex. 8.5) → "08:30", pour les infobulles (plus précis que fmtHour,
// qui arrondit à la demi-heure pour les repères de plage).
function fmtHeureMin(h) {
  const hh = Math.floor(h);
  const mm = Math.round((h - hh) * 60);
  return String(hh).padStart(2, '0') + ':' + String(mm).padStart(2, '0');
}

/**
 * Met à jour la carte de progression de filtration.
 * La timeline représente uniquement la plage de filtration (debut → fin).
 * - Segments colorés : historique des modes (AUTO=bleu, MANU ON=vert, MANU OFF=orange, OFF=gris, BOOST ON=violet, BOOST OFF=rouge)
 * - Trait blanc      : heure actuelle
 * - Repère vert      : fin théorique de filtration
 */
function updateFiltration(data) {
  const fait     = data.filtFait     != null ? data.filtFait     : 0;
  const objectif = data.filtObjectif != null ? data.filtObjectif : 0;
  const debut    = data.filtDebut    != null ? data.filtDebut    : 0;
  const fin      = data.filtFin      != null ? data.filtFin      : 24;

  const heureNow = new Date().getHours() + new Date().getMinutes() / 60;
  const reste    = Math.max(0, objectif - fait);

  // ── Chiffres clés ──────────────────────────────────────────
  setTextContent('filtStatObj',   formatHM(objectif));
  setTextContent('filtStatFait',  formatHM(fait));
  setTextContent('filtStatPlage', fmtHour(debut) + ' – ' + fmtHour(fin));

  // ── Bloc "Temps restant" : adapté selon le mode ────────────
  const labelEl = document.getElementById('filtRemainingLabel');
  const resteEl = document.getElementById('filtStatReste');
  if (data.antiGel) {
    if (labelEl) labelEl.textContent = '❄️ Mode continu';
    if (resteEl) { resteEl.textContent = 'GEL'; resteEl.style.color = '#00d4ff'; }
  } else if (data.canicule) {
    if (labelEl) labelEl.textContent = '🌡️ Mode continu';
    if (resteEl) { resteEl.textContent = 'CHALEUR'; resteEl.style.color = '#ff6b35'; }
  } else {
    if (labelEl) labelEl.textContent = '⏱️ Temps restant';
    if (resteEl) { resteEl.textContent = formatHM(reste); resteEl.style.color = '#f59e0b'; }
  }

  // ── Labels fixes : journée complète 0h – 12h – 24h ────────
  setTextContent('filtLabelStart', '0h');
  setTextContent('filtLabelMid',   '12h');
  setTextContent('filtLabelEnd',   '24h');

  // ── Conversion heure décimale → % sur la journée complète (0–24h) ──
  const toPercent = h => Math.min(100, Math.max(0, (h / 24) * 100));

  // ── Zone surlignée = plage de filtration configurée ────────
  const rangeEl = document.getElementById('filtRange');
  if (rangeEl) {
    rangeEl.style.left  = toPercent(debut) + '%';
    rangeEl.style.width = ((fin - debut) / 24 * 100) + '%';
  }

  // ── Marqueurs visuels début / fin de plage (traits bleus) ──
  // Masqués quand la plage couvre déjà toute la journée (anti-gel/canicule,
  // 0h-24h) : ils tomberaient exactement sur les repères fixes 0h/24h de
  // l'axe, créant un texte dédoublé (un plus clair par-dessus un plus
  // sombre) sans apporter d'information utile — "toute la journée" se
  // suffit à lui-même.
  const plageJourneeComplete = debut <= 0 && fin >= 24;
  const markStart    = document.getElementById('filtWindowStart');
  const markStartLbl = document.getElementById('filtWindowStartLabel');
  const markEnd      = document.getElementById('filtWindowEnd');
  const markEndLbl   = document.getElementById('filtWindowEndLabel');
  if (markStart) markStart.style.display = plageJourneeComplete ? 'none' : '';
  if (markEnd)   markEnd.style.display   = plageJourneeComplete ? 'none' : '';
  if (!plageJourneeComplete) {
    if (markStart)    markStart.style.left = toPercent(debut) + '%';
    if (markStartLbl) markStartLbl.textContent = fmtHour(debut);
    if (markEnd)       markEnd.style.left   = toPercent(fin)   + '%';
    if (markEndLbl)     markEndLbl.textContent   = fmtHour(fin);
  }

  // ── Segments de mode (bande du haut) : quel mode était sélectionné ──
  const timeline = document.getElementById('filtTimeline');
  const segs     = Array.isArray(data.modeHistory) ? data.modeHistory : [];

  // Supprimer les anciens segments (mode + pompe)
  if (timeline) {
    timeline.querySelectorAll('.mode-seg').forEach(el => el.remove());
    timeline.querySelectorAll('.pump-seg').forEach(el => el.remove());
    timeline.querySelectorAll('.seg-marker').forEach(el => el.remove());
  }

  if (segs.length > 0 && timeline) {
    segs.forEach(seg => {
      const segStart = seg.s;
      const segEnd   = seg.e < 0 ? heureNow : seg.e;  // -1 = en cours
      if (segStart >= 24 || segEnd <= 0) return;  // hors journée seulement

      const left  = toPercent(segStart);
      const right = toPercent(segEnd);
      if (right <= left + 0.1) return;  // segment trop étroit → ignoré

      const el = document.createElement('div');
      el.className        = 'mode-seg';
      el.style.left       = left + '%';
      el.style.width      = (right - left) + '%';
      el.style.background = MODE_SEG_COLORS[seg.t] || '#6b7280';
      el.title = (MODE_SEG_LABELS[seg.t] || '?') + ' : '
               + fmtHeureMin(segStart) + ' – ' + fmtHeureMin(segEnd);
      timeline.appendChild(el);

      // Petit repère au-dessus de la barre à chaque changement de mode (sauf
      // tout début de journée, sans intérêt à marquer).
      if (segStart > 0.01) {
        const marker = document.createElement('div');
        marker.className = 'seg-marker';
        marker.style.left = left + '%';
        timeline.appendChild(marker);
      }
    });
  }

  // ── Marche réelle de la pompe (bande du bas, bleu clair) ────
  // Remplace l'ancien bloc "fait" synthétique (toujours ancré à l'heure de
  // début programmée, donc faux dès que la pompe tourne hors de ce créneau,
  // ex. via une marche forcée à cheval sur minuit) par les vrais instants
  // marche/arrêt suivis côté firmware (`historique_pompe` dans main.rs).
  const pumpSegs = Array.isArray(data.pumpHistory) ? data.pumpHistory : [];
  if (pumpSegs.length > 0 && timeline) {
    pumpSegs.forEach(seg => {
      if (seg.t !== 1) return;  // seules les périodes "pompe en marche" sont dessinées
      const segStart = seg.s;
      const segEnd   = seg.e < 0 ? heureNow : seg.e;
      if (segStart >= 24 || segEnd <= 0) return;

      const left  = toPercent(segStart);
      const right = toPercent(segEnd);
      if (right <= left + 0.1) return;

      const el = document.createElement('div');
      el.className   = 'pump-seg';
      el.style.left   = left + '%';
      el.style.width  = (right - left) + '%';
      el.title = 'Pompe en marche : ' + fmtHeureMin(segStart) + ' – ' + fmtHeureMin(segEnd);
      timeline.appendChild(el);
    });
  }

  // ── Curseur heure actuelle (trait blanc + heure affichée au-dessus) ──
  const cursorEl = document.getElementById('filtCursor');
  if (cursorEl) {
    cursorEl.style.left = toPercent(heureNow) + '%';
  }
  const cursorLbl = document.getElementById('filtCursorLabel');
  if (cursorLbl) {
    const maintenant = new Date();
    cursorLbl.textContent = String(maintenant.getHours()).padStart(2, '0') + ':'
                           + String(maintenant.getMinutes()).padStart(2, '0');
  }

  // ── Fin théorique de filtration (trait vert) ───────────────
  const finTheorique = debut + objectif;
  const endEl        = document.getElementById('filtEnd');
  const endLbl       = document.getElementById('filtEndLabel');
  if (endEl) {
    endEl.style.display = (fait >= objectif || finTheorique >= fin) ? 'none' : 'block';
    endEl.style.left    = toPercent(finTheorique) + '%';
  }
  if (endLbl) {
    const hh = Math.floor(finTheorique);
    const mm = Math.round((finTheorique - hh) * 60);
    endLbl.textContent = String(hh).padStart(2, '0') + 'h'
                       + (mm > 0 ? String(mm).padStart(2, '0') : '');
  }
}

/** Convertit des heures décimales en "Xh YYmn" */
function formatHM(hours) {
  const h = Math.floor(hours);
  const m = Math.round((hours - h) * 60);
  return m > 0 ? h + 'h ' + String(m).padStart(2, '0') + 'mn' : h + 'h';
}


// ============================================================
// 6. GESTION DES DÉFAUTS
// ============================================================

/**
 * Met à jour la carte "Défauts Système".
 * @param {string[]} faults - Liste des messages de défaut actifs
 */
function updateFaults(faults) {
  const card       = document.getElementById('cardFaults');
  const list       = document.getElementById('faultList');
  const noFaultMsg = document.getElementById('noFaultMsg');
  if (!card || !list || !noFaultMsg) return;

  list.innerHTML = '';

  if (faults.length === 0) {
    card.classList.remove('has-faults');
    card.classList.add('no-faults');
    noFaultMsg.style.display = 'block';
  } else {
    card.classList.remove('no-faults');
    card.classList.add('has-faults');
    noFaultMsg.style.display = 'none';
    faults.forEach(msg => {
      const li = document.createElement('li');
      li.className = 'fault-item';
      const dot = document.createElement('span');
      dot.className = 'fault-dot';
      li.appendChild(dot);
      li.appendChild(document.createTextNode(msg));
      list.appendChild(li);
    });
  }
}


// ============================================================
// 6. GESTION DE LA PERTE DE CONNEXION (Bandeau)
// ============================================================

function showConnectionError() {
  let banner = document.getElementById('connection-error-banner');
  if (!banner) {
    banner = document.createElement('div');
    banner.id = 'connection-error-banner';
    banner.style.cssText = [
      'position:fixed', 'top:0', 'left:0', 'right:0',
      'background:#cc0000', 'color:#fff', 'text-align:center',
      'padding:10px', 'font-weight:bold', 'z-index:9999',
      'font-size:0.85rem', 'letter-spacing:1px',
    ].join(';');
    banner.textContent = '⚠  Connexion perdue avec l\'ESP32 — Nouvelle tentative en cours…';
    document.body.prepend(banner);
  }
  banner.style.display = 'block';
}

function hideConnectionError() {
  const banner = document.getElementById('connection-error-banner');
  if (banner) banner.style.display = 'none';
}


// ============================================================
// 7. PLAGE HORAIRE DE FILTRATION
// ============================================================

let schedStart = 8;
let schedEnd   = 20;

function fmtHour(h) {
  const hInt = Math.floor(h);
  return hInt + (h % 1 !== 0 ? 'h30' : 'h00');
}

async function loadSchedule() {
  try {
    const res = await fetch('/schedule');
    const d   = await res.json();
    schedStart = d.start;
    schedEnd   = d.end;
    document.getElementById('schedStart').textContent = fmtHour(schedStart);
    document.getElementById('schedEnd').textContent   = fmtHour(schedEnd);
    document.getElementById('schedNote').textContent  =
      'Enregistré : ' + fmtHour(schedStart) + ' – ' + fmtHour(schedEnd);
  } catch (e) {
    document.getElementById('schedNote').textContent = 'Erreur de chargement.';
  }
}

function adjustHour(field, delta) {
  if (field === 'start') {
    schedStart = Math.round((Math.max(0, Math.min(23.5, schedStart + delta))) * 2) / 2;
    document.getElementById('schedStart').textContent = fmtHour(schedStart);
  } else {
    schedEnd = Math.round((Math.max(0.5, Math.min(24, schedEnd + delta))) * 2) / 2;
    document.getElementById('schedEnd').textContent = fmtHour(schedEnd);
  }
}

async function saveSchedule() {
  if (schedStart >= schedEnd) {
    document.getElementById('schedNote').textContent =
      'Erreur : l\'heure de début doit être inférieure à la fin.';
    return;
  }
  try {
    const res = await fetch('/set-schedule?start=' + schedStart + '&end=' + schedEnd);
    if (res.ok) {
      document.getElementById('schedNote').textContent =
        'Enregistré : ' + fmtHour(schedStart) + ' – ' + fmtHour(schedEnd);
    } else {
      document.getElementById('schedNote').textContent = 'Erreur : valeurs rejetées.';
    }
  } catch (e) {
    document.getElementById('schedNote').textContent = 'Erreur réseau.';
  }
}

// ============================================================
// 8. DURÉE DU BOOST (marche / arrêt forcé)
// ============================================================

let boostDurMin = 60;

/** Convertit des minutes en "XhYY" ou "XXmin"  ex: 90 → "1h30", 30 → "30min" */
function fmtBoostDur(min) {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h > 0 ? h + 'h' + String(m).padStart(2, '0') : min + 'min';
}

async function loadBoostDuration() {
  try {
    const res = await fetch('/sensors');
    const d   = await res.json();
    if (d.boostDuration != null) {
      boostDurMin = d.boostDuration;
      const durEl  = document.getElementById('boostDurValue');
      const noteEl = document.getElementById('boostDurNote');
      if (durEl)  durEl.textContent  = fmtBoostDur(boostDurMin);
      if (noteEl) noteEl.textContent = 'Enregistré : ' + fmtBoostDur(boostDurMin);
    }
  } catch (e) {
    const noteEl = document.getElementById('boostDurNote');
    if (noteEl) noteEl.textContent = 'Erreur de chargement.';
  }
}

function adjustBoostDuration(delta) {
  boostDurMin = Math.max(30, Math.min(480, boostDurMin + delta));
  const durEl = document.getElementById('boostDurValue');
  if (durEl) durEl.textContent = fmtBoostDur(boostDurMin);
}

async function saveBoostDuration() {
  try {
    const res    = await fetch('/set-boost-duration?minutes=' + boostDurMin);
    const noteEl = document.getElementById('boostDurNote');
    if (res.ok) {
      if (noteEl) noteEl.textContent = 'Enregistré : ' + fmtBoostDur(boostDurMin);
    } else {
      if (noteEl) noteEl.textContent = 'Erreur : valeur rejetée.';
    }
  } catch (e) {
    const noteEl = document.getElementById('boostDurNote');
    if (noteEl) noteEl.textContent = 'Erreur réseau.';
  }
}

// ============================================================
// 8. EFFACEMENT DES JOURNAUX
// ============================================================

function openClearModal() {
  document.getElementById('clearModal').style.display = 'flex';
}

function closeClearModal() {
  document.getElementById('clearModal').style.display = 'none';
}

async function confirmClearLogs() {
  closeClearModal();
  try {
    const res = await fetch('/clear-logs');
    if (res.ok) {
      document.getElementById('logDisplay').textContent = 'Journaux effacés.';
      document.querySelectorAll('.btn-log').forEach(b => b.classList.remove('active'));
      document.getElementById('btnLogClose').style.display = 'none';
    } else {
      document.getElementById('logDisplay').textContent = 'Erreur lors de l\'effacement.';
    }
  } catch (e) {
    document.getElementById('logDisplay').textContent = 'Erreur réseau.';
  }
}

// ============================================================
// 8. UTILITAIRE
// ============================================================

function setTextContent(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
}


// ============================================================
// 8. LANCEMENT DES TÂCHES CADENCÉES
// ============================================================

updateDateTime();
fetchAllData();
loadSchedule();
loadBoostDuration();

const clockInterval   = setInterval(updateDateTime, 1000);
const sensorsInterval = setInterval(fetchAllData,   2000);
