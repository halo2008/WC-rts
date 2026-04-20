# WC-rts — postępy prac

> Snapshot stanu implementacji względem [`implementation-plan.md`](./implementation-plan.md). Aktualizuj ten plik na koniec każdej większej rundy. Data: **2026-04-20**.

## Stan ogólny

| Etap | Zakres | Status |
|-----:|--------|--------|
| 1 | Fundament — Strategic Map + game-core | ~95% ✅ |
| 2 | Battlefield Instance + Building | ~85% ✅ |
| 3 | Nations + Units + Realtime Movement + Economy + Grupy (porcja A) | ~90% ✅ |
| 4 | FoW + RTS Combat | ~25% 🔶 (combat auto-resolve wpięty; FoW i per-unit kompat nierozpoczęte) |
| 5 | Supply / Morale / Drony | 0% |
| 6 | Dyplomacja / Sojusze | 0% |
| 7 | Rynek / Nukes / Weather | 0% |
| 8 | Polish / Multiplayer / Launch | 0% |

**MVP (Etapy 1–4) ukończone w ~74%.** Demo end-to-end działa — gracz wybiera państwo, spawnuje jednostki, grupuje je w armie HoI-style, widzi płynny tranzyt z serwera via WebSocket, wchodzi w walkę na współdzielonym heksie (auto-resolve). Polar (flat-earth) projekcja działa jako overview. Brakuje FoW, per-unit combatu, gładkich coastlines i prawdziwego map data (obecny terrain to procedural Earth-like blobs).

---

## Etap 1 — Fundament

### Zrobione
- Cargo workspace: `crates/game-core`, `crates/game-core-wasm`, `crates/game-server`, `tools/seed`
- `game-core/src/hex.rs` — hex math + `distance_wrap` (poprawne min-z-3-kandydatów)
- `game-core/src/terrain.rs` — 11 typów, `FromStr`/`as_str()`, `movement_cost`, `is_passable_land`
- `game-core/src/grid.rs` — `StrategicGrid` z viewport query i wrap-around. Fixed bugs:
  - `row_pitch = 1.5 * hex_size` (było `2.0 * hex_size` = flat-top)
  - Per-row offset zakresu Q (wcześniej odcinało mapę diagonalnie)
  - `ViewportCell { cell, render_q }` — render_q może być poza `[0, width)` żeby wrap-around kopie były prawidłowo pozycjonowane
- **Earth-like terrain generator** — 15 Gaussowskich blob'ów przybliżających kontynenty (NA, SA, Grenlandia, Europa, Skandynawia, Sahara, Afryka, Middle East, Azja, Syberia, India, SE Asia, Australia). Antarktyda jako pełny pas `lat < -65`. Pasy pustynne per-lon (Sahara/Arabia/Gobi/Outback/Kalahari). Noise łamie okręgi → coastline organiczny.
- `game-core/src/pathfinding.rs` — A* wrap-aware + `astar_limited` + `reachable_hexes` + testy edge-case
- `game-core/src/spatial.rs` — `HexIndex<T>` + `line_of_sight` + `line_of_sight_elevated`
- `game-core-wasm/` — bindings dla wszystkich strategic + battlefield calls; `thread_local! RefCell<Option<_>>` zamiast UB z `OnceLock + unsafe`
- `docker-compose.yml` — Postgres 16+PostGIS, Redis 7, NATS JetStream
- `crates/game-server` — Axum + sqlx + tokio, migrations, health endpoints, tracing
- `tools/seed` — `StrategicGrid::new(1200, 600)` → batch insert do `hex_map` (720k wierszy) z earth-like terrain
- `.github/workflows/ci.yml` — CI dla cargo + wasm-pack + pnpm
- `apps/web/app/components/HexMap.tsx` — PixiJS renderer:
  - **Polar (flat-earth) projection** z toggle P / przycisk w HUD. Biegun N pośrodku, S na obwodzie. `projectPolar(wx, wy)` + adaptive stride.
  - Continent edges (coast + ice wall) — detektor granic land/water i ice/rest, batched `stroke()` per kolor.
  - Capital markers — nation color dot + label kodu (USA/CHN/POL/BRA).
  - Batched `fill()` per kolor (10-100× mniej GPU state changes niż per-hex).
  - Wrap-around multi-copy rendering (tile mapy gdy zoom-out < szerokość ekranu).
  - Zoom-to-cursor w piksel-space (nie przez wrapped hex) żeby nie skakało przy wrap.
  - Camera / Pixi cleanup: `destroy({ removeView: true }, true)` + explicit DOM child removal.
- `apps/web/app/components/Minimap.tsx` — bottom-left overlay, click-to-navigate, viewport rect z offsetem center-based
- `apps/web/app/lib/wasm-loader.ts` + `wasm.worker.ts` — cache-bust `?v=timestamp` w dev, `module_or_path` object API (fix deprecated warning)

### Niedokończone
- [ ] **1.7.B** Map Data Pipeline (NASA SRTM / Natural Earth) — obecnie procedural Earth-like, user prosi o dokładne kontynenty
- [ ] **1.3.F** 5-level LOD renderer + tile system
- [ ] **1.5.F** Instanced WebGL hex mesh / texture-based renderer (polar wolno dla 720k hex)
- [ ] **1.6.F** Nation borders rendering (shader-based z DB)
- [ ] **1.4.B** HPA* hierarchical pathfinding
- [ ] **1.9.F** WebLLM PoC `/lab/llm`
- [ ] **Contour tracing** coastlines (marching squares — gładkie linie zamiast obecnych prostopadłych kresek)

---

## Etap 2 — Battlefield + Building

### Zrobione
- `game-core/src/battlefield/` — `BattlefieldGenerator::generate()` z FBM noise + biome z macro hex; `BattlefieldInstance` z 21 typami budynków
- `game-core/src/building/` — `types.rs` (21 typów z HP/power/build_time/color/label), `construction.rs` (queue), `fortification.rs`, `extraction.rs`
- Migracja `002_battlefield.sql` — `battlefield_instances`, `buildings`, `construction_orders`, `resource_deposits`
- Server routes: `GET /api/battlefield/:q/:r`, `POST /build`, `GET /buildings`, `GET /deposits`, `GET /viewport`
- `apps/web/app/components/BattlefieldMap.tsx` — PixiJS renderer battlefield z budynkami
- `apps/web/app/page.tsx` — state switcher strategic ↔ battlefield (dbl-click na land hex)

### Niedokończone
- [ ] **2.2.F** zoom transition animation (Pixi zoom+fade ~400ms)
- [ ] **2.3.F** typed route `/game/[sessionId]/hex/[q]/[r]`
- [ ] **2.4.F** breadcrumb navigation
- [ ] **2.5.F** radial build menu
- [ ] **2.7.F** drag-to-place dla fortyfikacji

---

## Etap 3 — Nations + Units + Realtime + Economy + Grupy

### Zrobione
- `game-core/src/economy/resource.rs` — `Resource` (Fuel/Metals/Tech/Food), `Stockpile`
- `game-core/src/military/unit.rs` — `UnitType` × 6 z `UnitStats`, `UnitDomain`, `speed_on(terrain)`, `Stack::from_units`
- `game-core/src/military/movement.rs` — `Transit` z per-leg terrain-aware advance + testy
- `game-core/src/military/group.rs` — **UnitGroup** (HoI-style army group). `speed_on(&[UnitType], Terrain)` bierze najwolniejszego członka + testy
- `game-core/src/nation.rs` — `Nation`, `NationState`, `BudgetAllocation`
- Migracje:
  - `003_nations_units.sql` — `nation_state`, `units`, `unit_transits`, `production_queue` + seed 4 państw
  - `004_unit_groups.sql` — `unit_groups`, `units.group_id`, `group_transits`
- Server routes:
  - `GET /api/nations` (teraz z `capital_q/capital_r`) / `/:id` / `/:id/state` / `PATCH /:id/budget`
  - `GET /api/units` / `POST` / `GET /:id` / `POST /:id/move` / `GET /:id/transit` / `POST /:id/cancel`
  - `GET /api/units/production` / `POST`
  - `GET /api/groups` / `POST` / `GET /:id` / `POST /:id/disband` / `POST /:id/move` / `POST /:id/cancel` / `POST /:id/add` / `POST /:id/remove`
- `crates/game-server/src/tick.rs` — strategic tick 1 Hz:
  - `advance_transits` — każdy `Active`/`Blocked` transit o 1s; broadcast `WsMessage::Transit`
  - `advance_group_transits` — grupa porusza się jako jedność, snap wszystkich członków do current hex; broadcast `WsMessage::GroupTransit`
  - `advance_production` — tick każdego `InProgress` order; po ukończeniu INSERT `units` + broadcast `WsMessage::UnitSpawn`
- `crates/game-server/src/economy.rs` — tick 5-min z upkeep + base production + stability drift
- `crates/game-server/src/ws.rs` — `/ws/strategic` hub z `broadcast::Sender<WsMessage>`
- `crates/game-server/src/messages.rs` — `WsMessage::{Transit, NationState, UnitSpawn, GroupUpdate, GroupDisbanded, GroupTransit, Combat, UnitDeath}`
- Frontend:
  - `NationPicker.tsx`, `ResourceHUD.tsx`, `GameScreen.tsx`
  - Panel grupy z listą jednostek, Rozwiąż, Anuluj ruch
  - Przycisk "🪖 Grupuj heks" — tworzy grupę z wszystkich wolnych jednostek na heksie wybranej jednostki
  - Capital markers z kodem nacji na mapie
  - `unit_death` handle — usuwa z listy, koryguje `member_unit_ids` grupy
  - `combat` handle — toast "⚔ Twoje jednostki walczą (q,r)"
  - Units + grupy widoczne w polarze (min radius 5 / 9 px)

### Niedokończone
- [ ] **3.2.B** JSON config ~40 państw (obecnie hardcode 4 w migracji)
- [ ] **Morale cascade** per-unit przy deficycie
- [ ] **3.4.F** Budget allocation UI
- [ ] **3.5.F** Unit sprites battlefield
- [ ] **3.6.F** Selection UI battlefield + ctrl+1-9 groups
- [ ] **3.9.F** Production queue UI
- [ ] **3.10.B** game-core-wasm bindings dla unit/movement (client-side prediction)
- [ ] **Grupy porcja B** — shift+click multi-select, merge 2 grup, split (wyjmij jednostki), custom nazwa

---

## Etap 4 — FoW + RTS Combat (w trakcie)

### Zrobione
- `game-core/src/combat.rs` — `autoresolve(hex, side_a, side_b, terrain) -> CombatOutcome` z `CombatUnit`, `UnitDamage`, `terrain_cover()`. DAMAGE_COEFF=0.4, cover bonus dla underdog. Testy: empty side, bigger hurts smaller, cover helps underdog, damage distribution, destroyed flag
- **`crates/game-server/src/combat.rs`** — `resolve_combats()` wywoływane z `tick::tick_once` po `advance_group_transits`:
  - Query hexy z ≥2 nacjami (`COUNT DISTINCT nation_id`)
  - Pairwise `autoresolve` między każdą parą nacji (3 nacje → 3 walki, damage kumuluje)
  - UPDATE `units.hp` lub DELETE gdy hp≤0
  - Auto-disband grup które straciły ostatniego członka
  - Broadcast `WsMessage::Combat` (per hex) + `WsMessage::UnitDeath` (per dead unit)

### Niedokończone
- [ ] **4.1.B–4.3.B** FoW domain + Redis bitset + radar/detection
- [ ] **4.1.F–4.3.F** FoW shader PixiJS (3 stany: unknown/explored/visible)
- [ ] **4.4.B** Battlefield state machine CALM↔ACTIVE
- [ ] **4.5.B–4.7.B** Per-unit combat (range, LOS, armor, flanking), projectile system, unit order queue
- [ ] **4.8.B** State delta compression (bincode binary)
- [ ] **4.11.B** Auto-resolve UI (klient wybiera zamiast wchodzić na battlefield)
- [ ] **4.5.F** Battle HUD (C&C style unit info panel + minimap + ability bar)
- [ ] **4.14.F** Combat alert na strategic map (popup z lossami, fade-out anim dla zabitych)

---

## Nowy kierunek — **Cities & Industrial Zones** (pre-Etap 5)

> User priority, 2026-04-20: wokół dużych miast będzie skupiona produkcja przemysłowa, fabryki stawiane w tych obszarach.

### Do zrobienia
- [ ] **Real-world map data** — pobrać Natural Earth 10m raster (landmass + land elevation), sample per hex w seed tool. Obecny procedural blob'a generator zastąpić prawdziwymi granicami kontynentów. ~1-2 rundy.
- [ ] **Cities dataset** — Top ~100 major cities (Tokyo, NYC, Shanghai, London, Mumbai, SP, LA, Moskwa, etc.) z (lat, lon, population, nation_code). Hardcoded JSON w `tools/seed/data/cities.json`. Mapping na (q, r) przez lat/lon → hex.
- [ ] **`cities` table** — (id, name, nation_id, hex_q, hex_r, population, tier). Migracja `005_cities.sql`. Capital sheet już przechowuje jedno miasto-stolicę per nacja; cities rozszerza do wszystkich.
- [ ] **Industrial zones** — każde miasto produkuje bonus zasobów per tick; na hexach w promieniu 2-3 od miasta można stawiać fabryki (bonus do produkcji).
- [ ] **Frontend** — miasto jako marker (mniejszy od stolicy, proporcjonalny rozmiar do tier), kliknięcie w miasto pokazuje stats. Industrial zone overlay highlight.
- [ ] **Economy integration** — base production per nacja = sum(city.output) + bonus z fabryk. Zastąpić obecne `tier-based` base production.

### Architektura (proponowana)
```
tools/seed/data/
  landmask.png  (Natural Earth, 1200×600 equirectangular, land/water binary)
  cities.json   (lista miast z lat/lon/pop/nation)

crates/game-core/src/
  world/
    mod.rs
    city.rs     // City, IndustrialZone, city_radius_bonus()
  
crates/game-server/src/
  cities.rs     // endpoints GET /api/cities, POST /api/cities/:id/factory
  migrations/005_cities.sql
```

---

## Etap 5–8 — nierozpoczęte

Pełny scope — patrz `implementation-plan.md`.

---

## Historyczne bugi (naprawione)

| # | Bug | Fix |
|---|-----|-----|
| 1 | `line_of_sight_elevated` wywoływało `from.line_to(from)` | `line_to(to)` + 3 testy |
| 2 | `Hex::distance_wrap` miało komentarz "Approximate" i błędną geometrię | min z 3 kandydatów (direct/+width/−width) + 4 testy |
| 3 | WASM `BATTLEFIELD: OnceLock<_>` + `unsafe` cast do `*mut` | `thread_local! RefCell<Option<_>>` — zero unsafe |
| 4 | Parsery duplikowane w 3 miejscach (server + WASM + seed) | `FromStr`/`as_str()` w game-core |
| 5 | Server miał własną `generate_deposits_for_terrain` rozjeżdżającą się z generatorem | Server używa `BattlefieldGenerator::generate()` z game-core |
| 6 | Minimap viewport rect od top-left zamiast center | Offset `-vw/2, -vh/2` |
| 7 | Martwy `"game-core-wasm": "file:../../packages/game-core-wasm"` w package.json | Usunięte — WASM z `public/wasm/` |
| 8 | Podwójne `enqueueProduction` w `queueArmor` | Pojedyncze wywołanie |
| 9 | Viewport `hex_to_pixel(cell.hex, ...)` ignorował `q_col` → wrap kopie odcinane | `Hex::new(q_col, r)` + `ViewportCell { render_q }` |
| 10 | `normalizeScreenX` single-wrap → hexy znikały gdy mapa < viewport | `wrappedScreenXs(worldX, halfSpan)` zwraca wszystkie widoczne kopie |
| 11 | Zoom-to-cursor snapował do canonical position blisko anty-merydianu | Liczone w piksel-space: `camX_new = world.x * scale - (sx - vw/2)` |
| 12 | Pointy-top viewport używał `hex_h = 2.0 * size` (flat-top pitch) + stały zakres Q | `row_h = 1.5 * size` + per-row `offset = r / 2` |
| 13 | `app.destroy(true)` nie usuwało dzieci ani canvas → HMR nakładał warstwy, mapa "pływała" | `destroy({ removeView: true }, true)` + manual DOM cleanup |
| 14 | Pixi v8 `beginPath()` resetuje całą ścieżkę → batch-per-kolor rysował tylko ostatni hex (mapa czarna) | `appendHexToPath()` bez beginPath; jeden `beginPath` per bucket |
| 15 | Capital markers na wierzchu unit markers → spawnowane jednostki w stolicy niewidoczne | Layer order: capitals pod units |

---

## Demo runtime

- Frontend: http://localhost:3000 (Next.js 16)
- Backend: http://localhost:8080 (`game-server` release, `PORT=8080`)
- Infra: Docker Compose (postgres/redis/nats)
- Mapa: 720k hexów z earth-like terrain (4 migracje przelecą przy starcie)
- Państwa: 4 (USA, CHN, POL, BRA) — wszystkie 4 stolice na lądzie w nowym terrainie

Start po padzie:
```bash
docker compose up -d postgres redis nats
source "$HOME/.cargo/env"
PORT=8080 DATABASE_URL=postgres://game:game_dev@localhost:5432/grand_strategy \
  RUST_LOG=game_server=info /home/konrad-sedkowski/code/WC-rts/target/release/game-server &
cd apps/web && NEXT_PUBLIC_GAME_API=http://localhost:8080 NEXT_PUBLIC_GAME_WS=ws://localhost:8080 pnpm dev &
```

Po zmianie terrain generatora w `game-core`:
```bash
# Rebuild game-core + seed + WASM
cargo build --release && wasm-pack build crates/game-core-wasm --target web --out-dir ../../apps/web/public/wasm --out-name game_core_wasm
# Re-seed DB
psql ... -c "TRUNCATE hex_map;"
./target/release/seed
# Restart server + F5 frontend
```

---

## Logiczny next step (2026-04-20)

**User priority**: dokładne odwzorowanie kontynentów + major cities / industrial zones.

### Rundy

1. **Real map data pipeline** (1-2 rundy)
   - Pobierz Natural Earth land/ocean raster 1200×600 lub niżej (≤ 1 MB PNG)
   - Dodaj `image` crate do `tools/seed/Cargo.toml`
   - W `seed.rs` sample pixel per (q, r) → klasyfikuj terrain (land vs water + latitude dla ice)
   - Rebuild, re-seed, restart
   - **Efekt**: rozpoznawalne kształty Afryki, Indii, Skandynawii, Włoch — zamiast obecnych blobów

2. **Cities pipeline** (1 runda)
   - Dodaj `tools/seed/data/cities.json` — top ~100 miast z lat/lon/population/nation_code (JSON hardcoded)
   - Migracja `005_cities.sql` — tabela `cities`
   - W seed po terrain: insert `cities` z computed (q, r)
   - Endpoint `GET /api/cities`
   - Frontend: layer markerów miast (proporcjonalny rozmiar do tier), label przy zoom-in
   - **Efekt**: widać Nowy Jork, Szanghaj, Warszawę na mapie

3. **Industrial zones + factories** (1-2 rundy)
   - `IndustrialZone` w domain — city + promień 2-3 hex
   - `POST /api/cities/:id/factory` — stawia fabrykę (building) w zone
   - Economy: sumuj city output + factory bonus per nacja
   - UI: klik miasto → panel stats, widoczne "industrial zone" highlights

4. **Potem**: FoW (Etap 4.1-4.3) albo contour tracing krawędzi (polish wizualny).

### Perf note
Polar projection renderuje 720k hexów per tick (CPU-side). Docelowo pre-rendered `RenderTexture` + GPU UV mapping — osobna duża runda, po cities. Obecnie używalne przy domyślnym zoomie.
