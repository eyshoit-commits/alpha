# Telemetrie-Betriebshandbuch

Version: 0.2 · Zuletzt aktualisiert: 2025-10-19

---

## 1. Überblick
- Der `cave-daemon` initialisiert seine Telemetrie über `telemetry::init` (`crates/cave-daemon/src/telemetry.rs`).
- Standardmäßig wird ein konsolenbasiertes Logging mit OTLP-Export kombiniert.
- Sampling wird über `CAVE_OTEL_SAMPLING_RATE` (Float 0.0–1.0) gesteuert; ungültige Werte werden auf `[0.0, 1.0]` geclamp’t und als Warning ausgegeben.
- Bei Fehlern im OTLP-Setup fällt der Daemon automatisch auf Console-Logs zurück und meldet den Fehler via `warn!`.
- `TelemetryGuard` sorgt beim Herunterfahren für ein Flush der OTEL-Provider (inkl. Fehler-Logging bei Problemen).

---

## 2. Konfiguration

| Variable | Beschreibung | Default |
|----------|--------------|---------|
| `CAVE_OTEL_SAMPLING_RATE` | Float-Wert zwischen 0.0 und 1.0. Werte >1.0 werden auf 1.0, Werte <0.0 auf 0.0 gesetzt. | `1.0` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP-gRPC Endpoint (z. B. `http://otel-collector:4317`). | `http://localhost:4317` |
| `OTEL_EXPORTER_OTLP_HEADERS` | Kommagetrennte Auth-/Routing-Header (`key=value`). | leer |
| `OTEL_EXPORTER_OTLP_TIMEOUT` | Timeout in Sekunden für Exporter. | 10 |

Weitere OTLP-Optionen werden automatisch über die Standard-Environment-Variablen des `opentelemetry-otlp` Crates geladen (`OTEL_EXPORTER_OTLP_*`).

**Hinweise:**
- Ein Sampling-Wert von `0.0` deaktiviert den OTLP-Exporter (nur Console-Logs).
- Sampling-Werte >0.0 initialisieren einen Batch-Span-Processor mit Tokio-Runtime (`install_batch(Tokio)`).
- Beim Wechsel des Sampling-Werts ist kein Neustart erforderlich; die Änderung wird beim nächsten Prozessstart übernommen.

---

## 3. Betriebsabläufe

1. **Collector-Überwachung**
   - Prüfe `/metrics` des Daemons und des OTEL-Collectors.
   - `warn!("failed to initialize OTEL exporter")` deutet auf ein Setup-Problem hin (Endpoint/Netzwerk).
2. **Sampling-Reviews**
   - Verifiziere pro Umgebung: Dev `1.0`, Staging `0.5`, Prod `0.05–0.2` (siehe `docs/governance.md`).
   - Notiere Anpassungen in `docs/Progress.md` mit Datum/Owner.
3. **Shutdown/Rotation**
   - Vor Wartungsfenstern den Collector erreichbar halten, damit `TelemetryGuard` sauber flushen kann.
   - Bei Collector-Rotation neue Endpoints/Headers via Env-Variablen setzen.

---

## 4. Tests & Validierung

| Zweck | Kommando |
|-------|----------|
| Sampling-Parser | `cargo test -p cave-daemon telemetry::tests` |
| End-to-End (lokal, mit Collector) | `CAVE_OTEL_SAMPLING_RATE=0.5 OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317 cargo run -p cave-daemon --bin cave-daemon` |

**Automatische Checks:**
- Unit-Tests stellen sicher, dass Sampling-Werte korrekt geclamp’t/parst werden (`crates/cave-daemon/src/telemetry.rs`).
- Bei aktivem OTLP-Endpoint prüft `telemetry::init`, ob der Exporter initialisiert werden kann; Fehler erzeugen einen Warn-Logeintrag und blockieren den Start nicht.

---

## 5. Troubleshooting

| Symptom | Ursache | Maßnahmen |
|---------|---------|-----------|
| Warnung: `failed to initialize OTEL exporter; continuing with console logs only` | Collector nicht erreichbar oder falscher Endpoint/Header | Endpoint/Firewall prüfen, Env-Variablen aktualisieren, Dienst neu starten. |
| Warnung: `CAVE_OTEL_SAMPLING_RATE='<value>' ...` | Ungültiger Sampling-Wert | Wert im Deployment-Config anpassen (0.0–1.0). |
| Keine Traces im Backend, obwohl keine Warnung | Sampling-Wert zu niedrig oder Collector filtert | Sampling erhöhen (z. B. 0.2), Collector-Config prüfen. |

---

## 6. Referenzen
- `crates/cave-daemon/src/telemetry.rs`
- `docs/governance.md` – Vorgaben für Sampling je Umgebung
- `docs/Progress.md` – Nachverfolgung von Telemetrie-/Governance-Änderungen
