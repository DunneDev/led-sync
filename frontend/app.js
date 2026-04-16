
// =========================================================================
// CONFIGURATION
// =========================================================================
const MQTT_HOST = 'q22312e0.ala.us-east-1.emqxsl.com';
const MQTT_PORT = 8084;
const MQTT_USERNAME = 'web-led';
const MQTT_PASSWORD = 'YnqjasLV^4Ki$9';
const THROTTLE_MS = 150;
// =========================================================================

const TOPIC_BUTTON = 'button';
const TOPIC_LED = 'led';

// DOM
const $connBadge = document.getElementById('conn-badge');
const $connLabel = document.getElementById('conn-label');
const $stateBadge = document.getElementById('state-badge');
const $stateText = document.getElementById('state-text');
const $hwStage = document.getElementById('hw-stage');
const $hwOuter = document.getElementById('hw-outer');
const $hwInner = document.getElementById('hw-inner');
const $log = document.getElementById('event-log');
const $orb = document.getElementById('led-orb');
const $hex = document.getElementById('led-hex');
const $rgb = document.getElementById('led-rgb');
const $rR = document.getElementById('range-r');
const $rG = document.getElementById('range-g');
const $rB = document.getElementById('range-b');
const $vR = document.getElementById('val-r');
const $vG = document.getElementById('val-g');
const $vB = document.getElementById('val-b');
const $throttleWrap = document.getElementById('throttle-wrap');
const $throttleBar = document.getElementById('throttle-bar');

// ── Helpers ───────────────────────────────────────────────────────
const toHex = v => v.toString(16).padStart(2, '0').toUpperCase();
const pad2 = n => String(n).padStart(2, '0');
const nowTs = () => { const d = new Date(); return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`; };

function setConn(state, label) {
  $connBadge.className = 'conn-badge ' + state;
  $connLabel.textContent = label;
}

function logEvent(type, msg) {
  if ($log.children.length === 1 && !$log.children[0].querySelector('.log-msg.press, .log-msg.release')) {
    $log.innerHTML = '';
  }
  const row = document.createElement('div');
  row.className = 'log-row';
  row.innerHTML = `<span class="log-ts">${nowTs()}</span><span class="log-msg ${type}">${msg}</span>`;
  $log.prepend(row);
  while ($log.children.length > 40) $log.removeChild($log.lastChild);
}

function setButtonState(pressed) {
  $hwStage.classList.toggle('pressed', pressed);
  $hwOuter.classList.toggle('pressed', pressed);
  $hwInner.classList.toggle('pressed', pressed);
  $stateBadge.className = `state-badge ${pressed ? 'pressed' : ''}`;
  $stateText.textContent = pressed ? 'Pressed' : 'Released';
  logEvent(pressed ? 'press' : 'release', pressed ? 'Button pressed' : 'Button released');
}

function updateLedPreview(r, g, b) {
  $vR.textContent = r; $vG.textContent = g; $vB.textContent = b;
  const col = `rgb(${r},${g},${b})`;
  const glow = `rgba(${r},${g},${b},0.45)`;
  $orb.style.background = col;
  $orb.style.boxShadow = `0 0 16px ${glow}, 0 2px 6px rgba(0,0,0,0.10)`;
  $hex.textContent = `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  $rgb.textContent = `rgb(${r}, ${g}, ${b})`;
}

let lastSent = 0;
let pendingTimer = null;
let barTimer = null;

function animateBar() {
  // Show and animate the throttle bar across THROTTLE_MS
  $throttleWrap.classList.add('active');
  $throttleBar.style.transition = 'none';
  $throttleBar.style.width = '0%';
  // force reflow
  $throttleBar.offsetWidth;
  $throttleBar.style.transition = `width ${THROTTLE_MS}ms linear`;
  $throttleBar.style.width = '100%';
  clearTimeout(barTimer);
  barTimer = setTimeout(() => {
    $throttleWrap.classList.remove('active');
    $throttleBar.style.transition = 'none';
    $throttleBar.style.width = '0%';
  }, THROTTLE_MS + 80);
}

function publishLed(r, g, b) {
  if (!client || !client.connected) return;
  const now = Date.now();
  const wait = THROTTLE_MS - (now - lastSent);

  clearTimeout(pendingTimer);

  if (wait <= 0) {
    client.publish(TOPIC_LED, JSON.stringify({ r, g, b }), { qos: 0, retain: true });
    lastSent = Date.now();
    animateBar();
  } else {
    pendingTimer = setTimeout(() => {
      const rr = +$rR.value, gg = +$rG.value, bb = +$rB.value;
      client.publish(TOPIC_LED, JSON.stringify({ r: rr, g: gg, b: bb }), { qos: 0, retain: true });
      lastSent = Date.now();
      animateBar();
    }, wait);
  }
}

function onSliderChange() {
  const r = +$rR.value, g = +$rG.value, b = +$rB.value;
  updateLedPreview(r, g, b);
  publishLed(r, g, b);
}

[$rR, $rG, $rB].forEach(el => el.addEventListener('input', onSliderChange));
updateLedPreview(0, 255, 0);

const PRESETS = [
  [255, 0, 0], [255, 100, 0], [255, 220, 0], [0, 200, 50],
  [0, 180, 255], [30, 80, 255], [160, 0, 255], [255, 255, 255], [0, 0, 0]
];

const $presetsEl = document.getElementById('presets');
PRESETS.forEach(([r, g, b]) => {
  const s = document.createElement('div');
  s.className = 'preset-swatch';
  s.style.background = `rgb(${r},${g},${b})`;
  if (!r && !g && !b) s.style.border = '2px solid #d1d5db';
  s.title = `R ${r}  G ${g}  B ${b}`;
  s.addEventListener('click', () => {
    $rR.value = r; $rG.value = g; $rB.value = b;
    onSliderChange();
  });
  $presetsEl.appendChild(s);
});

// MQTT
const client = mqtt.connect(`wss://${MQTT_HOST}:${MQTT_PORT}/mqtt`, {
  clientId: 'web_' + Math.random().toString(36).substr(2, 8),
  username: MQTT_USERNAME,
  password: MQTT_PASSWORD,
  reconnectPeriod: 5000,
  connectTimeout: 8000,
  clean: true,
});

client.on('connect', () => {
  setConn('connected', 'Connected');
  client.subscribe(TOPIC_BUTTON, { qos: 0 });
  client.subscribe(TOPIC_LED, { qos: 0 });
});

client.on('reconnect', () => setConn('', 'Reconnecting'));
client.on('offline', () => setConn('error', 'Offline'));
client.on('error', err => { setConn('error', 'Error'); console.error(err); });

client.on('message', (topic, payload) => {
  try {
    const data = JSON.parse(payload.toString());
    if (topic === TOPIC_BUTTON) {
      if (data.msg === 'pressed') setButtonState(true);
      if (data.msg === 'released') setButtonState(false);
    }
    if (topic === TOPIC_LED && data.r !== undefined) {
      const r = Math.min(255, Math.max(0, Math.round(data.r)));
      const g = Math.min(255, Math.max(0, Math.round(data.g)));
      const b = Math.min(255, Math.max(0, Math.round(data.b)));
      $rR.value = r; $rG.value = g; $rB.value = b;
      updateLedPreview(r, g, b);
    }
  } catch (e) { console.warn('Bad payload', e); }
});
