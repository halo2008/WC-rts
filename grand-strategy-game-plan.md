# Grand Strategy RTS - Plan Projektu

## Wizja gry

Przeglądarkowa gra strategiczna w czasie rzeczywistym z **dwupoziomową mapą**:

1. **Strategic Map** — płaska hex mapa globu w stylu logo ONZ (equirectangular projection, **1200x600 hex ≈ 720 000 heksów**, każdy hex ≈ 33km). Widać tu tylko stacki armii, konwoje, granice państw, supply lines, alerty radarowe, wojny, dyplomację. Zoom-out = cała Ziemia.
2. **Sector Map (Operational/Tactical)** — gdy klikniesz region, zoomujesz do konkretnego sektora z drobną siatką. **Variable sizing: 512x512 hex standard, 1024x1024 dla strategicznych sektorów** (stolice, fronty, zasoby). Tu budujesz bazy, fortyfikacje, rozstawiasz jednostki, i TU odbywa się RTS bitwa gdy wróg wejdzie.

Hybryda grand strategy (Victoria 3, Hearts of Iron) z taktycznym RTS (C&C Generals) + geoscape/battlescape model z X-COM. Gracz zarządza państwem, buduje sojusze, prowadzi ekonomię i OSOBIŚCIE dowodzi bitwami w sektorach.

**USP**: Jedyna przeglądarkowa gra gdzie gracz steruje jednostkami w bitwie jak w C&C, a wynik zależy od micro-skilla, nie auto-resolve. Sektory są **persistent** — co zbudujesz zostaje.

**Filozofia**: Zero pay-to-win. Gra niszowa dla maniaków strategii. Wygrywasz mózgiem, nie portfelem. Złożoność to feature, nie bug. 500 stron wiki i gracze ją przeczytają.

**Monetyzacja**: Organiczna — cosmetics, supporter tier, one-time purchase. Żadnych timerów, gemów, pay-to-skip.

---

## Zasady architektoniczne

### Stack: Rust + WASM + PixiJS + Next.js 16

**Rdzeń gry jest w Rust.** Jeden `game-core` crate zawiera całą logikę domenową (hex math, pathfinding, battle sim, economy tick, morale, supply). Crate kompiluje się do:

- **`wasm32-unknown-unknown`** → załadowany w przeglądarce, napędza frontend (deterministyczna predykcja, rendering state)
- **`x86_64-unknown-linux-gnu`** → backend Axum (authoritative game state, persist w PostgreSQL)

**Dlaczego tak:**
- **Performance**: 720k hex strategic + 1M hex sektor + A* pathfinding + battle 20Hz + economy tick = TypeScript by się dławił. Rust robi to bez zastanowienia.
- **Determinizm**: multiplayer authoritative wymaga deterministycznej symulacji. Rust + fixed-point math = replay-ready, desync-proof.
- **Shared logic**: ten sam kod liczy bitwę na backendzie (authoritative) i na frontendzie (client prediction) — zero driftu mechaniki między frontem a backiem.
- **Type safety**: Rust type system > TS, mniej bugów w złożonej domain logic (morale, supply, combat matrix).

**Next.js 16 (React 19, Turbopack default, typed routes)** — shell UI: panele dyplomacji, dashboard ekonomii, budget allocation, diplomacy screens, lobby, auth. Gdzie potrzebny SSR i React ecosystem.

**PixiJS (WebGL)** — renderery map: strategic (instanced hex rendering + LOD tiling), sector (sprite'y jednostek, budynków, efekty), battle scenes. PixiJS dostaje state z Rust/WASM i rysuje.

**Czego NIE używamy:**
- **NestJS/TypeScript backend** — odrzucone, Rust backend wystarczy, mniejsza divergencja frontend↔backend
- **Bevy WASM** jako główny renderer — bundle 10MB+, PixiJS znacznie lżejszy i dojrzały dla 2D
- **Yew/Leptos/Dioxus** zamiast React — traci ecosystem UI, słaby DX dla paneli

### Hexagonal Architecture (Ports & Adapters) — WSZĘDZIE

Cały kod projektu stosuje architekturę heksagonalną. Gra o heksach powinna mieć heksagonalny kod. Ułatwia:

- **Testowanie**: domain logic bez infra dependencies (unit testy bez DB)
- **Wymianę technologii**: zmień DB, message bus, renderer — domain nietknięte
- **Edycję mechanik**: game designer zmienia reguły w domain layer bez dotykania infra
- **Modding potential**: gracze mogą podmienić adaptery (custom AI, custom balance)

```
Każdy moduł (w game-core crate i w backend crate) ma identyczną strukturę:

  module_name/
  │
  ├── domain/                    # RDZEŃ — czysta logika, no_std friendly
  │   ├── model/                 # Entity, Value Object, Aggregate
  │   │   ├── unit.rs            # Unit entity (morale, supply, veterancy)
  │   │   ├── hex.rs             # Hex value object (q, r, terrain)
  │   │   └── battle.rs          # Battle aggregate
  │   ├── port/                  # Traits (abstrakcje)
  │   │   ├── inbound.rs         # Driving ports (use cases)
  │   │   └── outbound.rs        # Driven ports (repository, publisher)
  │   ├── service/               # Domain services (orchestracja reguł)
  │   │   ├── movement.rs
  │   │   ├── combat.rs
  │   │   └── supply.rs
  │   └── event/                 # Domain events
  │       ├── unit_moved.rs
  │       └── battle_resolved.rs
  │
  ├── application/               # Use cases — orchestracja, transakcje
  │   ├── command/               # CQRS commands
  │   └── query/                 # CQRS queries
  │
  └── adapter/                   # Implementacje traitów — INFRASTRUKTURA
      ├── inbound/               # Axum handlers, WebSocket handlers, WASM bindings
      └── outbound/              # sqlx (Postgres), redis, nats/pubsub, external APIs
```

**Zasada żelazna**: `domain/` NIGDY nie importuje z `adapter/`. Zależności płyną DO WEWNĄTRZ. `domain/` jest `no_std`-friendly gdzie to możliwe — żeby ten sam kod leciał na WASM i na natywnym backendzie.

```
Przykład — zmiana mechaniki morale:
  PRZED: Morale spada -5/h w radiacji
  PO:    Morale spada -5/h × (1 - nbc_protection) w radiacji
  Zmiana TYLKO w: domain/service/morale.rs
  Automatycznie idzie na backend + frontend (shared crate).

Przykład — zmiana bazy danych:
  PRZED: PostgreSQL → PO: CockroachDB
  Zmiana TYLKO w: adapter/outbound/persistence/

Przykład — dodanie nowego typu drona:
  1. domain/model/drone.rs        → dodaj wariant do enum
  2. domain/service/combat.rs     → dodaj damage rules
  3. game-config/drones.json      → dodaj stats
  GOTOWE.
```

### Game Config jako dane, nie kod

Wszystkie statystyki, koszty, damage tables, unit stats — **JSON/YAML**, nie hardcoded. Ładowane przez port `GameConfigPort` (trait), adapter deserializuje z pliku przez serde.

```
packages/game-config/
  ├── units/              # infantry, armor, drones, naval
  ├── buildings/          # factories, defenses, infrastructure
  ├── economy/            # resources, trade, market
  ├── combat/             # damage matrix, terrain, morale factors
  ├── diplomacy/          # relations, alliances, alignment
  ├── nations/            # 40+ nation configs
  └── radiation/          # zones, NBC protection, fallout
```

Config jako sygnatura modów: gracze mogą podmienić pliki i zmienić balans bez dotykania kodu. W przyszłości: Steam Workshop / mod browser.

### Local-First Development

Cały stos działa na `docker compose up` — zero chmury, zero kont, zero płacenia:

```
docker-compose.yml:

  # Infrastruktura
  postgres:     PostgreSQL 16 + PostGIS, port 5432
  redis:        Redis 7, port 6379
  nats:         NATS JetStream, port 4222         # event bus (lekki, nie Pub/Sub)

  # Backend (jeden proces na start, monolith-first)
  game-server:  Rust/Axum, port 3000              # API + WebSocket + battle sim

  # Frontend
  web:          Next.js 16, port 8080             # SSR shell + PixiJS

  # Dev tools
  pgadmin:      pgAdmin 4, port 5050
  redis-insight: RedisInsight, port 8001
```

**Zasada**: jeśli potrzebujesz konta GCP żeby odpalić `cargo run` + `pnpm dev` — coś jest źle.

Etapy deployment:
```
Etap LOCAL (teraz):
  → docker compose up → wszystko na localhost
  → NATS zamiast Cloud Pub/Sub, local Postgres, jeden Rust proces
  → Zero kosztów, zero kont, zero konfiguracji chmurowej

Etap STAGING (gdy 20-50 alpha testerów):
  → Jeden VPS (Hetzner €20/miesiąc)
  → PostgreSQL managed lub na VPS

Etap PROD (gdy 500+ graczy):
  → K8s (GKE Autopilot lub Hetzner managed)
  → Rozbicie Rust backendu na shardy per-game-session (każda partia = własny process/pod)
  → Cloud SQL (managed Postgres) + Memorystore (Redis) + NATS cluster
  → CDN na tile'e mapy (pre-rendered LOD layers)
```

### Rust Monolith-First z modułami

Na start NIE rozbijamy na mikroserwisy. Jeden Rust binary (Axum) z modułami:

```
services/game-server/
  src/
    map/                    # MapModule (strategic hex map + sectors)
    economy/                # EconomyModule
    military/               # MilitaryModule (movement, supply, morale)
    detection/              # DetectionModule (FoW, radar)
    diplomacy/              # DiplomacyModule
    state/                  # StateModule (nation, society, alignment)
    battle/                 # BattleModule (RTS tick w sektorach)
    matchmaking/            # MatchmakingModule (lobby, assignment)

    main.rs                 # Composition root, DI przez konstruktory
```

Wszystko wywala się do `game-core` crate jako domain/application; `game-server` to tylko adapter wiring (Axum + sqlx + redis + nats).

Dzięki hexagonal arch: każdy moduł jest GOTOWY do wyciągnięcia jako osobny proces. Porty się nie zmieniają. Tylko adapter zmienia się z in-process call na NATS message.

---

## Architektura technologiczna

### Stack decyzje

| Warstwa | Technologia | Dlaczego |
|---------|------------|----------|
| **Frontend — shell/UI** | Next.js 16 (App Router, React 19) | SSR paneli, dashboard, dyplomacja, lobby, auth |
| **Frontend — game logic** | Rust → WASM (`game-core`) | Deterministyczna logika, shared z backendem, fast |
| **Frontend — mapa strategic** | PixiJS (WebGL instanced) | 720k hex rendering, LOD tiling, płynny zoom |
| **Frontend — mapa sector** | PixiJS (WebGL) | 512k-1M hex, sprite'y, sektory, RTS renderowanie |
| **Backend — API + game loop** | Rust + Axum + Tokio | Jeden stack z frontend core, performance, typy |
| **Backend — battle tick** | `game-core` w Rust (tick 20Hz w sektorze) | Authoritative, deterministic, in-process |
| **Backend — economy tick** | Tokio scheduled task | Tick co 5 min, batch operations |
| **Baza danych** | PostgreSQL 16 + PostGIS | Spatial queries, transakcje, persistent world |
| **Cache / Hot state** | Redis | FoW bitsets, ceny rynkowe, session, active battle state |
| **Message Bus** | NATS JetStream | Event-driven między modułami/shardami (lżejszy niż Pub/Sub) |
| **Realtime** | WebSocket (Axum ws) + binary frames (bincode/msgpack) | Map updates, battle sync — binary, nie JSON |
| **Infra** | K8s + Terraform + Helm | Auto-scaling, per-game shardy |
| **Auth** | OAuth2 (Zitadel albo keycloak) + JWT | Single source of truth |
| **CI/CD** | GitHub Actions + cargo-chef (cached Rust builds) | Szybkie build'y |
| **Monitoring** | Prometheus + Grafana + Loki | Metryki, alerty, game analytics |

### Dlaczego nie TypeScript backend

- **Performance-critical**: grand strategy + RTS + 720k hex + battle 20Hz = TS by się dławił
- **Shared crate**: logika gry jeden raz, frontend (WASM) + backend konsumują ten sam kod → zero divergencji mechaniki
- **Determinizm**: Rust + fixed-point → multiplayer desync-proof
- **Jeden język compute**: Rust na całość logiki. TS tylko do shell UI (Next.js), gdzie blyszczy

### Architektura runtime

```
┌─────────────────────────────────────────────────────────────┐
│                         FRONTEND                             │
│                                                              │
│  Next.js 16 (shell UI)                                       │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐        │
│  │Dashboard │ │Diplomacy │ │ Lobby    │ │ Auth     │        │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘        │
│                                                              │
│  PixiJS renderery (WebGL)                                    │
│  ┌────────────────────┐ ┌────────────────────┐              │
│  │ Strategic Renderer │ │  Sector Renderer   │              │
│  │ (1200x600 hex LOD) │ │  (512-1024 hex)    │              │
│  └──────────┬─────────┘ └──────────┬─────────┘              │
│             │                       │                        │
│  ┌──────────▼───────────────────────▼─────────┐             │
│  │  game-core WASM (Rust)                      │             │
│  │  hex math · pathfinding · battle sim ·      │             │
│  │  client prediction · state reconciliation   │             │
│  └──────────────────────┬──────────────────────┘             │
└─────────────────────────┼────────────────────────────────────┘
                          │ WebSocket (binary frames)
┌─────────────────────────┼────────────────────────────────────┐
│                         ▼                                     │
│               Rust game-server (Axum + Tokio)                │
│  ┌────────────────────────────────────────────────────┐     │
│  │  API routes · WebSocket hub · Auth · Rate limit    │     │
│  └────────────────────────────────────────────────────┘     │
│                                                              │
│  Domain modules (game-core crate):                           │
│  ┌──────┬─────────┬──────────┬──────────┬─────────────┐     │
│  │ Map  │ Economy │ Military │Detection │ Diplomacy   │     │
│  └──────┴─────────┴──────────┴──────────┴─────────────┘     │
│  ┌──────────────────────────┬─────────────────────────┐     │
│  │ Battle (tick 20Hz/sector)│  AI nations             │     │
│  └──────────────────────────┴─────────────────────────┘     │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  Infrastructure                                      │    │
│  │  PostgreSQL · Redis · NATS · GCS/S3 (tiles)         │    │
│  └─────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────┘
```

**Skalowanie:** jeden game session = jeden `game-server` process. Shardy per sesja (lobby matchmaker przydziela). Strategic tick 1Hz, active sector battles tick 20Hz (tylko te w których dzieje się walka).

---

## Dwupoziomowa mapa — szczegóły

### Strategic Map (global)

- **Rozmiar**: 1200x600 hex (~720k heksów), equirectangular projection, ONZ-style
- **Jeden hex ≈ 33km** (kompromis między Polska-jako-1-hex a każdy-hex-to-miasto)
- **Wrap-around** na osi X (scroll wschód → Pacyfik → Ameryki seamlessly)
- **Polar clamp** na osi Y (biegun = jeden hex, dystorcja uznana jako feature, nie bug)
- **Co widać**:
  - Stacki armii (ikony, NATO lub sprite), kolorowane przez nation
  - Granice państw (wektorowe, na shaderze)
  - Supply lines (animowane strzałki)
  - Radar contacts (blipy, FoW-respecting)
  - Trade routes, konwoje
  - Nuclear launches (trail), fallout plumes
  - Weather overlay, radiation overlay
- **Rendering**:
  - Instanced WebGL hex mesh — jeden draw call na wszystkie hexy w view
  - 5-level LOD: planet → region → sector grid → hex detail → unit detail
  - Max zoom-out: shader koloruje hex-y na GPU po nation ownership, zero sprite'ów
  - Spatial index (hex-hash w Rust) do query "co widać w viewport"
- **Persistent state**: ownership, infrastruktura, resource deposits, pogoda

### Sector Map (operational/tactical)

- **Klikasz hex na strategic → zoom do sektora** (pojedynczy strategic hex może mieć powiązany sektor)
- **Rozmiar variable**:
  - 512x512 hex — standardowy sektor (prowincja, region)
  - 1024x1024 hex — kluczowe sektory (stolice, fronty, zasoby)
- **Jeden hex sektora ≈ 50-100m** (micro-tactical scale)
- **Co widać**:
  - Teren szczegółowy (rzeki, wzgórza, lasy, drogi, mosty)
  - Budynki i bazy (twoje + widoczne wroga)
  - Fortyfikacje (trenches, bunkry, minefields)
  - Jednostki pojedyncze (nie stacki)
  - RTS-style controls (selekcja, grupy, rozkazy)
- **Rendering**:
  - PixiJS 2D top-down (opcjonalnie izometric view dla FAZY 2)
  - Sprite'y jednostek, animowane (idle/walk/attack/die)
  - Particle effects (eksplozje, dym, tracery)
  - FoW shader, night/day, pogoda
- **Persistent state**: co zbudujesz zostaje. Sektor = mała, zawsze-on instancja świata.
- **Tryby sektora**:
  - **Calm**: tick 1Hz, tylko production/construction progresses
  - **Active combat**: tick 20Hz gdy wróg w sektorze, RTS real-time
- **Pathfinding**: hierarchical (HPA* albo JPS+) — 1M hex sektor compute'ny w <10ms

### Macro ↔ Micro transition

```
Gracz widzi stack armii w strategic hex (row 123, col 456)
→ klika sektor → UI "Enter sector?"
→ loading (ściąga sektor state + decompress na Rust/WASM, ~200ms)
→ smooth zoom-in animation
→ gracz gra w sektorze (buduje, rozkazuje)
→ ESC / Strategic View → zoom-out, sektor tick-uje dalej w tle

Wróg wchodzi do twojego sektora:
→ Alert na strategic map ("Combat in Warsaw Sector")
→ Auto-resolve opcja (szybka bitwa, losowy wynik ważony siłami)
→ Command opcja → wchodzisz w sektor, RTS real-time
→ Active combat tick 20Hz, tylko ten sektor
→ Koniec walki → merge survivors do strategic stacka
```

---

## Etapy wdrożenia

Projekt podzielony na 8 etapów. Każdy etap = grywalna wersja z przyrostową funkcjonalnością.

---

### ETAP 1: Fundament — Strategic Map + `game-core` crate (6-8 tygodni)

**Cel**: Gracz widzi globalną mapę świata (1200x600 hex), scrolluje, zoomuje, widzi granice państw.

#### 1.1 Monorepo setup
- **Stack**: Cargo workspace (Rust) + pnpm workspace (Node)
- **Klocki**:
  - [ ] Monorepo: `crates/game-core`, `crates/game-server`, `apps/web`, `packages/game-config`
  - [ ] Cargo workspace + pnpm workspace, shared root
  - [ ] Docker Compose (postgres, redis, nats, pgadmin)
  - [ ] CI (GitHub Actions): cargo check/test/clippy/fmt, pnpm lint/test/build
  - [ ] cargo-chef caching w CI dla szybkich builds
  - [ ] wasm-pack config dla `game-core` → WASM bundle

#### 1.2 game-core crate (domain layer)
- **Stack**: Rust (no_std-friendly gdzie można)
- **Klocki**:
  - [ ] Axial coordinate system (q, r) z wrap-around na osi X (modulo 1200)
  - [ ] Hex math: distance, neighbors, LOS, ring, spiral, wrap-aware
  - [ ] A* pathfinding (wrap-aware) — generic nad `Hex` trait (strategic i sector reużywają)
  - [ ] HPA* hierarchical pathfinding dla dużych sektorów (1M hex)
  - [ ] Terrain types enum (DEEP_OCEAN, COAST, PLAINS, FOREST, MOUNTAIN, DESERT, TUNDRA, URBAN...)
  - [ ] Spatial index (hex-hash, R-tree) dla query "co w viewport / w zasięgu"
  - [ ] Fixed-point arithmetic dla deterministycznych calculations (gdzie potrzebne do multi)
  - [ ] Unit testy edge cases (wrap boundary, polar clamp, LOS through mountains)
  - [ ] WASM bindings (wasm-bindgen) — eksponuj hex math do JS

#### 1.3 Map Data Pipeline
- **Stack**: Python + GDAL + Rust seed tool
- **Klocki**:
  - [ ] Import NASA SRTM heightmap → resample do 1200x600
  - [ ] Import Natural Earth coastlines → rasteryzacja na hex grid
  - [ ] Terrain classification (elevation + latitude + biome → terrain type)
  - [ ] Ręczna korekta chokepoints (Suez, Panama, Gibraltar, Bosphorus, Malacca)
  - [ ] Resource placement (geopolitycznie poprawne: ropa w Golf, uran w Kazachstan, etc.)
  - [ ] Pre-render LOD tile layers (5 poziomów) → GCS/CDN (binary format, PixiJS loader)
  - [ ] Seed tool w Rust: czyta outputy pipeline → insert do Postgres

#### 1.4 Map Service (game-server module)
- **Stack**: Axum + sqlx + PostGIS
- **Klocki**:
  - [ ] Hex batch queries (read-heavy, cached)
  - [ ] `GET /api/map/tiles/:lod/:x/:y` — tile endpoint (binary response)
  - [ ] `GET /api/map/hex/:q/:r` — single hex info
  - [ ] `GET /api/map/region/:q/:r/:radius` — range query (wrap-aware)
  - [ ] Nation ownership per hex (efficient batch updates)
  - [ ] DB schema: `hex_map`, `nations`, `territories`
  - [ ] PostGIS spatial index na hex coordinates

#### 1.5 Frontend — Strategic Map Renderer
- **Stack**: Next.js 16 + PixiJS + game-core WASM
- **Klocki**:
  - [ ] PixiJS canvas mount w Next.js client component
  - [ ] Tile loading system (slippy-map pattern, async fetch + cache)
  - [ ] 5-level LOD (planet → continent → region → hex grid → detail)
  - [ ] Camera controls (WASD/edge scroll, wheel zoom, kinetic inertia)
  - [ ] Hex grid: instanced WebGL mesh (jeden draw call, shader koloruje)
  - [ ] Seamless wrap-around na osi X
  - [ ] Nation borders rendering (wektorowe, shader-based)
  - [ ] Terrain coloring (per hex type, shader gradient)
  - [ ] Minimap widget (entire world overview, click-to-center)
  - [ ] game-core WASM loaded w Web Worker (pathfinding off main thread)

#### 1.6 WebLLM PoC — in-browser LLM (opcjonalny flavor text)
- **Stack**: WebLLM (MLC) / transformers.js + WebGPU, Web Worker
- **Cel**: wcześnie zweryfikować czy mały LLM w przeglądarce jest wykonalny na urządzeniach graczy — zanim zbudujemy całą warstwę AI nations/dyplomacji wokół.
- **Klocki**:
  - [ ] Benchmark harness: 10 polskich promptów domenowych (alert wojskowy, oświadczenie prezydenta, raport z frontu, headline newsa, nazwa operacji)
  - [ ] Test 3 modele × 2 urządzenia: **Qwen 3 0.6B**, **Qwen 3 1.7B**, **Gemma 3 1B** na desktop + telefonie Konrada
  - [ ] Metryki: tokens/s, czas pierwszego tokena, rozmiar pobrania, poprawność gramatyczna polskiego (ręczny scoring 1-5), VRAM/RAM footprint
  - [ ] Constrained generation PoC — wymusić strukturę JSON (headline, body, sentiment) przez grammar/schema
  - [ ] Domyślny wybór: **Qwen 3 1.7B** (primary) + **Gemma 3 1B** (fallback, licencja-safe)
  - [ ] WebLLM loader w Web Workerze (osobno od game-core WASM worker)
  - [ ] IndexedDB cache wagi modelu (pobranie raz, potem offline)
  - [ ] Toggle w ustawieniach gry: "AI flavor text: on/off/auto" (auto = wykryj WebGPU + dość RAM)
  - [ ] Fallback szablonowy (randomized templates) gdy LLM off/niewspierany — gra MUSI działać bez LLM
  - [ ] Decyzja o fine-tuningu (LoRA na polskich przykładach) — yes/no na podstawie wyników benchmarku
- **Tradeoffy**:
  - Pierwszy download 0.5-1GB → gracze na 3G zapłaczą. Stąd toggle + fallback.
  - WebGPU nie wszędzie (stare Safari, starsze Android) → auto-detect + graceful degradation.
  - Determinizm: LLM **tylko** dla non-gameplay warstwy (flavor, newsy, nazwy). Decyzje AI nation ZOSTAJĄ w deterministycznym `game-core` (utility AI / BT).

**Deliverable etapu 1**: Przeglądarka wyświetla mapę świata 1200x600 z heksami, kolorami terenu, granicami państw. Płynny zoom od widoku globu do pojedynczego hex. Seamless wrap E-W. WASM działa, hex math na frontend = ten sam co na backend. **Plus**: strona `/lab/llm` z benchmarkiem 3 modeli po polsku, wiemy który LLM wchodzi do MVP (lub czy rezygnujemy na rzecz szablonów).

---

### ETAP 2: Sector Map + Zoom Transition + Base Building (6-8 tygodni)

**Cel**: Gracz zoomuje z globalnej do sektora, widzi teren szczegółowy, buduje budynki, fortyfikacje.

#### 2.1 Sector Data Model
- **Klocki**:
  - [ ] DB schema: `sectors` (id, strategic_hex_q, strategic_hex_r, size, seed, terrain_blob)
  - [ ] `sector_buildings` (sector_id, hex_q, hex_r, type, hp, level, owner_id)
  - [ ] `sector_units` (sector_id, unit_id, hex_q, hex_r, ...)
  - [ ] Variable sector sizes (512x512 standard, 1024x1024 strategic)
  - [ ] Lazy sector generation (genuj dopiero gdy gracz wejdzie po raz pierwszy, z seed)
  - [ ] Procedural terrain generation w Rust (deterministic z seed, fbm noise + biome z macro hex)

#### 2.2 Zoom Transition
- **Klocki**:
  - [ ] Strategic → Sector animation (Pixi zoom + fade, ~400ms)
  - [ ] Loading state gdy sektor wymaga fetchu (<200ms cel)
  - [ ] Sector scene w PixiJS (osobny Container, swap on transition)
  - [ ] URL routing: `/game/[sessionId]/sector/[sectorId]` (Next.js 16 route)
  - [ ] Breadcrumb: "Europe → Poland → Warsaw Sector"
  - [ ] ESC / "World view" button → zoom-out

#### 2.3 Building System
- **Klocki**:
  - [ ] Building types: Factory, Barracks, Power Plant, Supply Hub, Radar, Airfield, Port, Defense
  - [ ] Construction UI w sektorze (klik hex → menu → build)
  - [ ] Construction queue (timer, resource cost)
  - [ ] Building HP, upgradeable (level 1-5)
  - [ ] Power grid (budynki wymagają power, jak w C&C)
  - [ ] Sprite rendering per building type (placeholder: colored squares + label)

#### 2.4 Fortifications
- **Klocki**:
  - [ ] Trench, Bunker, Minefield, AA Battery, Anti-Tank Position
  - [ ] Placement na sector hex (z drag-to-place dla linii trenches)
  - [ ] Defense bonus per fortification type
  - [ ] Destructible (bitwa redukuje HP)

#### 2.5 Resource Extraction (basic)
- **Klocki**:
  - [ ] Resource deposits na sector hex (iron, oil, uranium, food)
  - [ ] Mine/Well/Farm buildings — produkują resource
  - [ ] Production feeds nation stockpile (globalna ekonomia)

**Deliverable etapu 2**: Gracz zoomuje do sektora, widzi teren, stawia budynki/fortyfikacje. Buildings persystują. Economy tick widzi production z sektorów.

---

### ETAP 3: Nations, Units, Movement (6-8 tygodni)

**Cel**: Gracz wybiera państwo, produkuje jednostki, przesuwa stacki po strategic i unity po sektorach.

#### 3.1 Nation System
- **Klocki**:
  - [ ] DB schema: `nations`, `nation_state` (budget, policies, government)
  - [ ] Nation selection screen (tier 1-4, z opisami)
  - [ ] Initial state per nation (start resources, territories, units)
  - [ ] Government types (democracy, authoritarian, theocracy, junta) — efekty
  - [ ] Budget allocation UI (mil/eco/research/social/intel sliders)

#### 3.2 Resource System
- **Klocki**:
  - [ ] Resource types: FUEL, METALS, TECH, FOOD + advanced (URANIUM, HYDROGEN later)
  - [ ] Per-nation ledger (production, consumption, stockpile) — globalny
  - [ ] Factory recipes (input → output, np. Metals+Fuel → Munitions)
  - [ ] Economy tick co 5 min (Tokio scheduled task, batch SQL)
  - [ ] Resource HUD (top bar, 4 ikony + delta)
  - [ ] Deficit alerts (WebSocket push)

#### 3.3 Unit System
- **Klocki**:
  - [ ] DB: `units` (id, type, owner, location: strategic_hex lub sector_hex, hp, supply)
  - [ ] Unit types (6 basic): Infantry, Armor, Artillery, Fighter, Destroyer, Transport
  - [ ] Stack abstraction: na strategic = army stack (composition), na sector = individual units
  - [ ] Production queue per barracks/factory (w sektorze)
  - [ ] Supply consumption (fuel, food, ammo per hour)
  - [ ] Unit icons na strategic (stack badge + count), sprites na sector
  - [ ] Selection UI (click, box-select, ctrl+1-9 groups) — tryb sektor

#### 3.4 Movement System
- **Klocki**:
  - [ ] Strategic movement: klik stack → klik hex → pathfind → transit event (ETA)
  - [ ] Sector movement: RTS-style (RMB move, A+click attack-move)
  - [ ] Cross-sector movement (leave sector → strategic transit → enter sector)
  - [ ] Pathfinding w Rust/WASM (client prediction) + server auth (reconcile)
  - [ ] Speed modifiers per terrain
  - [ ] Fuel consumption on movement

**Deliverable etapu 3**: Gracz wybiera USA/Chiny/Rosję, buduje jednostki w sektorach, przesuwa stacki po globie, wchodzi w sektor i steruje unitami jak w RTS. Ekonomia tickuje.

---

### ETAP 4: Fog of War + RTS Combat w sektorach (8-10 tygodni) ← CORE FEATURE

**Cel**: FoW działa. Armie na wrogim sektorze → RTS bitwa. Gracz osobiście dowodzi jak w C&C.

#### 4.1 Fog of War
- **Klocki**:
  - [ ] Vision radius per unit type (strategic i sector osobno)
  - [ ] Visibility bitset per player (strategic: 720k bits = 90KB, sector: per-active-sector)
  - [ ] Incremental FoW update (dirty flag per player)
  - [ ] Server NIGDY nie wysyła danych poza FoW (antycheat enforcement)
  - [ ] FoW shader w PixiJS (3 stany: unknown/explored/visible)

#### 4.2 Radar / Detection
- **Klocki**:
  - [ ] Radar buildings (range, detection capabilities per unit type)
  - [ ] Detection events ("Air contact, sector NE")
  - [ ] Alert system (WebSocket → UI)
  - [ ] Stealth mechanics (submarines, stealth drones)

#### 4.3 Battle Tick w Sektorze
- **Stack**: game-core battle module, Axum WebSocket streaming
- **Klocki**:
  - [ ] Sector state machine: CALM / MOBILIZED / ACTIVE_COMBAT
  - [ ] Tick rate dynamic: 1Hz calm → 20Hz active combat
  - [ ] Unit state machine (idle, moving, attacking, retreating, dead)
  - [ ] Combat calculation: range, LOS, armor, flanking (front 1x, side 1.5x, rear 2.5x), cover
  - [ ] Projectile system (travel time, miss chance)
  - [ ] Terrain effects (elevation = range bonus, forest = cover)
  - [ ] State delta compression (bincode binary frames, not JSON)
  - [ ] Per-player FoW filtering (widzisz tylko swoje)
  - [ ] Anti-cheat validation (server authoritative, odrzuca invalid input)
  - [ ] Battle end conditions (all enemy dead, rout, time limit)

#### 4.4 Auto-Resolve fallback
- **Klocki**:
  - [ ] Quick battle calculation (formula: strength + terrain + fortification)
  - [ ] Casualty distribution
  - [ ] Offered as opcja gdy gracz nie chce micro-manage (np. małe walki)

#### 4.5 Battle Frontend (PixiJS)
- **Klocki**:
  - [ ] Battle HUD (unit info, minimap, chat, ability bar)
  - [ ] Selection system (click, box-select, ctrl+1-9)
  - [ ] Order system (RMB move, A attack-move, S stop, H hold)
  - [ ] Client-side prediction (game-core WASM) + server reconciliation
  - [ ] Interpolacja pozycji (20Hz server → 60fps render)
  - [ ] HP bars, selection circles
  - [ ] Projectile visuals (tracery, explosions, particles)
  - [ ] Camera controls (WASD, edge scroll, zoom, spacebar center on selection)
  - [ ] Ability UI (Q/W/E/R hotkeys, cooldown indicators)

**Deliverable etapu 4**: CORE GAMEPLAY LOOP kompletny. Buduj armię, eksploruj mapę, wrogi stack wchodzi do twojego sektora → RTS real-time → wygrywasz/przegrywasz → wynik merguje do strategic.

---

### ETAP 5: Supply, Morale, Drony (6-8 tygodni)

**Cel**: Jednostki potrzebują supply. Głodne buntują się. Drony jako asymetryczna broń.

#### 5.1 Supply Chain
- **Klocki**:
  - [ ] Supply Hub, Depot, Truck units
  - [ ] Supply routes (pathfind Hub → Depot → Front)
  - [ ] Supply delivery per unit per hour (food, fuel, ammo, water)
  - [ ] Supply status 5 poziomów (FULL → ADEQUATE → LOW → CRITICAL → DEPLETED)
  - [ ] Route disruption (bombardment, interception, sabotage)
  - [ ] Encirclement detection (no path = ENCIRCLED)
  - [ ] Priority queue UI (kto je pierwszy)
  - [ ] Airdrop / helicopter / foraging emergency resupply
  - [ ] Ammo compatibility (NATO vs Eastern)
  - [ ] Supply heat map overlay

#### 5.2 Morale System
- **Klocki**:
  - [ ] Morale 0-100 per unit
  - [ ] States: FANATICAL → HIGH → STEADY → SHAKY → WAVERING → BREAKING → ROUTING → MUTINY
  - [ ] Factors: combat, supply, casualties, leadership, environment, fatigue, radiation
  - [ ] Mutiny events (violent, defection, desertion, passive, petition)
  - [ ] Morale cascade (panic spreads)
  - [ ] Officer rally ability, rotation, propaganda
  - [ ] Order refusal (unit odmawia wejścia w radiację / samobójczego ataku)

#### 5.3 Fatigue + Veterancy
- **Klocki**:
  - [ ] Fatigue (freshness 0-100, drain per activity)
  - [ ] Friendly fire chance at low fatigue
  - [ ] Rotation mechanic (frontline ↔ reserve)
  - [ ] Veterancy: CONSCRIPT → REGULAR → EXPERIENCED → VETERAN → ELITE → HARDENED
  - [ ] XP per battle, stat bonuses per rank
  - [ ] PTSD (max morale cap decreases)
  - [ ] Veteran as instructor (training bonus)
  - [ ] Conscription policy (volunteer → full mobilization → scraping)

#### 5.4 Drone Warfare
- **Klocki**:
  - [ ] TIER 1: FPV Kamikaze, Recon Quad, Jammer
  - [ ] TIER 2: UCAV Bayraktar, Loitering Munition, Naval USV, EW Drone
  - [ ] TIER 3: Stealth UCAV, Strategic UAV, Drone Swarm, Hypersonic Drone
  - [ ] Drone Workshop building (fast, cheap production)
  - [ ] FPV control minigame w bitwie (WASD sterowanie FPV drone)
  - [ ] Drone Swarm control UI (formation, behavior)
  - [ ] Counter-drone: jammers, AA, laser CIWS, EMP

#### 5.5 Advanced Battle Mechanics
- **Klocki**:
  - [ ] Flanking damage multiplier
  - [ ] Cover system (buildings, forests, trenches)
  - [ ] Suppression, rout, surrender w bitwie
  - [ ] Combined arms bonuses
  - [ ] Smoke screen, dig in, guided missile abilities
  - [ ] Artillery forward observer
  - [ ] Night combat (reduced vision, thermal)

**Deliverable etapu 5**: Bitwy mają głębię. Supply matters — odcięta armia się buntuje. Drony dają asymetryczną broń. Micro-skill ceiling wyraźny.

---

### ETAP 6: Dyplomacja, sojusze, zarządzanie państwem (6-8 tygodni)

**Cel**: Multiplayer grand strategy z sojuszami, zdradami, społeczeństwami.

#### 6.1 Diplomacy
- **Klocki**:
  - [ ] Bilateral relations (political, economic, military, trust, ideological)
  - [ ] Actions: trade deal, military aid, sanctions, embargo, ultimatum, war declaration
  - [ ] Casus belli system (no CB = massive penalties)
  - [ ] Trust (betrayal records never expire, decay logarithmically)
  - [ ] Third-party effects (sanctions on A → B sees opportunity)

#### 6.2 Alliances
- **Klocki**:
  - [ ] Types: non-aggression, trade, mutual defense, military alliance, federation
  - [ ] Cohesion 0-1
  - [ ] Roles: Leader, Founding, Committed, Frontline, Reluctant, Unreliable
  - [ ] Joint operations (propose, vote, national caveats)
  - [ ] Command structure (unified, delay per trust)
  - [ ] Resource sharing (bilateral, common market, lend-lease)
  - [ ] Fracture events (member attacked + no response → shatter)

#### 6.3 State Management
- **Klocki**:
  - [ ] Budget allocation (real-time)
  - [ ] War support (democracy only, initiator matters)
  - [ ] National policies (military doctrine, economic policy)
  - [ ] Elections, coups, revolutions
  - [ ] Stability (unrest → protests → rebellion)

#### 6.4 Society & Alignment
- **Klocki**:
  - [ ] Satisfaction (economic, security, freedom, pride, war weariness)
  - [ ] Attachment score (cultural, economic, historical bonds)
  - [ ] 5-phase alignment shift (grumbling → wavering → flirting → defection → integration)
  - [ ] Rally-around-the-flag
  - [ ] Propaganda / info warfare
  - [ ] Soft power tools

#### 6.5 AI Nations
- **Klocki**:
  - [ ] Personality (aggression, reliability, pragmatism, risk tolerance)
  - [ ] Diplomatic decision making
  - [ ] Military strategy
  - [ ] Economic management
  - [ ] Alignment decisions
  - [ ] Coalition formation against hegemon

**Deliverable etapu 6**: Pełna multiplayer grand strategy. Sojusze, zdrady, społeczeństwa, neutralne narody flippują.

---

### ETAP 7: Rynek, nukes, weather, hydrogen (6-8 tygodni)

**Cel**: Ekonomiczne warfare. Broń nuklearna. Pogoda.

#### 7.1 Dynamic Market
- **Klocki**:
  - [ ] Commodity price engine (supply/demand, disruption, speculation)
  - [ ] Price ≠ profit (droga ropa = drogie koszty)
  - [ ] Physical trade routes, chokepoint vulnerability
  - [ ] Embargo/sanctions with leakage
  - [ ] Secondary sanctions
  - [ ] Inflation, economic collapse stages
  - [ ] Market manipulation (stockpile dump)
  - [ ] Resource depletion + exploration

#### 7.2 Hydrogen Economy
- [ ] H2 as advanced resource (water + power → H2)
- [ ] Infrastructure (electrolyzer, pipeline, storage, tanker)
- [ ] H2 units: scramjet drone, fuel cell tank, H2 warship
- [ ] Thermobaric warhead (poor man's nuke)
- [ ] Late-game geopolitical shift (oil down, rare earth up)

#### 7.3 AA Defense
- [ ] Layered: Strategic ABM, Long SAM, Medium, Short, Point
- [ ] Intercept rate matrix
- [ ] SEAD missions

#### 7.4 Hypersonic Weapons
- [ ] HGV (Mach 15+, 5-15% intercept), Cruise Missile (Mach 5-8)
- [ ] Boost-phase ICBM interceptor
- [ ] ASAT (Kessler syndrome risk)
- [ ] Ambiguity (conventional vs nuclear warhead)

#### 7.5 Nuclear Weapons
- [ ] Tactical (1-10kT), Strategic ICBM (MIRV), SLBM
- [ ] Nuclear triad
- [ ] Launch UI (confirmation, type CONFIRM)
- [ ] Retaliation AI (launch on warning / impact / absorb)
- [ ] Nuclear winter (>20 warheads = global food -40%)
- [ ] Escalation ladder

#### 7.6 Radiation
- [ ] Ground vs air burst
- [ ] Zones: EXTREME → LOW
- [ ] Wind affects fallout drift
- [ ] Plume visualization
- [ ] NBC protection, decontamination
- [ ] Reactor meltdown (Chernobyl event)

#### 7.7 Weather
- [ ] Global wind model
- [ ] Types: clear, rain, storm, blizzard, sandstorm, fog
- [ ] Effects: visibility, movement, flight ops, fire spread
- [ ] Seasonal (winter frozen rivers, summer arctic passage)
- [ ] Forecasting (intel investment)

**Deliverable etapu 7**: Pełna głębia strategiczna. Ekonomiczne warfare. Nukes z konsekwencjami. Pogoda/radiacja wpływają na ops.

---

### ETAP 8: Polish, multiplayer, launch (6-8 tygodni)

**Cel**: Public beta.

#### 8.1 Multiplayer Infrastructure
- [ ] Lobby (create, join, settings)
- [ ] Player assignment (nation, alliance, team)
- [ ] Scenarios: Modern Day, Cold Start, Flashpoint, Collapse
- [ ] Spectator mode
- [ ] Reconnect (disconnect → AI takeover → return)
- [ ] Anti-cheat (server authoritative, rate limiting, deterministic replay)
- [ ] Chat (global, alliance, bilateral)
- [ ] Save/load (persistent games, multi-day)
- [ ] Horizontal scaling: każda game session = osobny Rust process, matchmaker przydziela

#### 8.2 UI/UX Polish
- [ ] Unified flow: strategic ↔ sector ↔ economy ↔ diplomacy
- [ ] Notification center
- [ ] Tutorial / onboarding
- [ ] Customizable hotkeys
- [ ] Sound (ambient, combat, Geiger, alerts)
- [ ] Dynamic music
- [ ] Responsive (desktop primary, tablet secondary)
- [ ] Accessibility (colorblind, screen reader dla non-map UI)

#### 8.3 Performance Optimization
- [ ] Frontend: WebGL batching, texture atlases, frustum culling
- [ ] Web Worker dla WASM game-core (off main thread)
- [ ] Backend: economy tick batch SQL, cache hot data
- [ ] WebSocket binary compression (bincode + lz4)
- [ ] Battle: entity pooling, spatial partitioning
- [ ] CDN tile caching
- [ ] Load test: 32 players, 50 concurrent active sectors, 5 battles

#### 8.4 Battle Replay
- [ ] Record deterministic inputs + seed
- [ ] Replay viewer (timeline, pause, speed)
- [ ] Share links
- [ ] Spectator cam in live battles

#### 8.5 Meta & Analytics
- [ ] Player profiles (stats, W/L, favorite nation)
- [ ] Game analytics
- [ ] Balance telemetry
- [ ] ELO leaderboard

**Deliverable etapu 8**: PUBLIC BETA.

---

## Struktura monorepo

```
grand-strategy/
├── Cargo.toml                     # Workspace root
├── pnpm-workspace.yaml
│
├── crates/
│   ├── game-core/                 # Domain logic (Rust, no_std friendly)
│   │   src/
│   │     map/                     # Strategic + sector hex engines
│   │     economy/
│   │     military/                # Combat, supply, morale
│   │     detection/               # FoW, radar
│   │     diplomacy/
│   │     state/                   # Nation, society
│   │     battle/                  # RTS tick
│   │     ai/                      # NPC nations
│   │     lib.rs
│   │
│   ├── game-core-wasm/            # WASM wrapper (wasm-bindgen)
│   │   src/lib.rs                 # JS bindings dla frontend
│   │
│   └── game-server/               # Backend binary (Axum)
│       src/
│         adapter/
│           inbound/               # Axum routes, WebSocket
│           outbound/              # sqlx, redis, nats
│         main.rs
│
├── apps/
│   └── web/                       # Next.js 16 frontend
│       app/                       # App Router
│         (game)/                  # Game routes
│           [sessionId]/
│             page.tsx             # Strategic view
│             sector/[sectorId]/page.tsx  # Sector view
│         (shell)/                 # Lobby, auth, profile
│       components/
│         map-strategic/           # PixiJS strategic renderer
│         map-sector/              # PixiJS sector renderer
│         battle-hud/
│         diplomacy/
│       lib/
│         wasm/                    # game-core-wasm loader, worker
│
├── packages/
│   ├── game-config/               # JSON configs (units, buildings, etc.)
│   └── shared-types/              # TS types generated from Rust (ts-rs)
│
└── tools/
    ├── map-pipeline/              # Python + GDAL (map data gen)
    └── seed/                      # Rust binary (seed Postgres)
```

---

## Baza danych — kluczowe tabele

```sql
-- ETAP 1-2
hex_map (q, r, terrain, elevation, resource, nation_id, infrastructure_level)
nations (id, name, government_type, tier)
sectors (id, strategic_q, strategic_r, size, seed, terrain_blob, generated_at)
sector_buildings (id, sector_id, hex_q, hex_r, type, level, hp, owner_id)

-- ETAP 3
resources (nation_id, type, production, consumption, stockpile)
units (id, type, owner_id, location_type, location_id, hp, supply_status, veterancy_xp)
production_queue (building_id, unit_type, progress, eta)
transit_events (unit_id, origin_sector, dest_sector, path, departure, eta)

-- ETAP 4
visibility_strategic (player_id, hex_bitset)  -- Redis
visibility_sector (player_id, sector_id, hex_bitset)  -- Redis, per-active-sector
radar_stations (id, sector_id, hex_q, hex_r, range)
battle_instances (id, sector_id, started_at, status, participants[])

-- ETAP 5
supply_hubs (id, sector_id, hex_q, hex_r, owner_id, radius)
supply_routes (source_hub, dest_hub, waypoints, status, throughput)
morale_log (unit_id, timestamp, morale, factors)  -- time series
drone_workshops (id, sector_id, hex_q, hex_r, tier, queue)

-- ETAP 6
diplomatic_relations (nation_a, nation_b, political, economic, military, trust)
alliances (id, name, type, leader_id, cohesion)
alliance_members (alliance_id, nation_id, role, commitment, joined_at)
treaties (id, type, parties[], status, signed_at, terms)
betrayal_records (betrayer_id, victim_id, type, timestamp, severity)
society_state (nation_id, satisfaction, unrest, alignment, bonds)

-- ETAP 7
commodity_prices (resource_type, price, timestamp)
trade_routes (seller_id, buyer_id, resource, volume, waypoints)
sanctions (imposer_id, target_id, type, effectiveness)
nuclear_arsenal (nation_id, type, count, location_sector)
radiation_zones (id, sector_id, hexes[], intensity, decay_rate, wind_drift)
weather (sector_id, type, wind_direction, wind_speed, timestamp)
```

---

## Estymacja zespołu

| Rola | Ilość | Odpowiedzialność |
|------|-------|------------------|
| Tech Lead / Rust Architect | 1 | Architektura, code review, game-core design |
| Rust Developer (Backend + Core) | 2-3 | game-core crate, Axum backend, battle tick |
| Frontend (React/PixiJS + WASM integration) | 2 | Next.js 16 shell, PixiJS renderery, WASM bindings |
| Game Designer | 1 | Balans, stats, mechaniki, playtesting |
| DevOps / Infra | 1 | K8s, Terraform, CI/CD, monitoring |
| Artist (2D) | 1 | Sprites, UI, map tiles, ikony |
| Sound Designer | 0.5 | SFX, muzyka (part-time/contract) |
| **RAZEM** | **8-9** | |

Dla solo/small team: etapy 1-4 da się zrobić w 2 osoby (fullstack Rust + frontend) w ~6-8 miesięcy, placeholder grafika (NATO symbols).

---

## Priorytety MVP

```
MUST HAVE (etapy 1-4):
  ✅ Strategic hex mapa (1200x600) z FoW
  ✅ Sector maps (512x512) z zoom transition
  ✅ 4 bazowe zasoby + produkcja
  ✅ 6 typów jednostek
  ✅ Base building w sektorach
  ✅ Ruch strategic + sector
  ✅ RTS combat w sektorach (CORE USP!)
  ✅ Auto-resolve fallback
  ✅ 2-8 graczy multiplayer

SHOULD HAVE (etap 5):
  ⬜ Supply chain
  ⬜ Morale / mutiny
  ⬜ Drony (TIER 1-2)
  ⬜ Veterancy

NICE TO HAVE (etapy 6-7):
  ⬜ Dyplomacja / sojusze / society alignment
  ⬜ Dynamic market / sanctions
  ⬜ Nukes / radiation
  ⬜ Weather / hydrogen economy
```

**Etapy 1-4 = GRYWALNA GRA. Reszta to głębia.**

---

## Ryzyka techniczne

| Ryzyko | Prawd. | Mitygacja |
|--------|--------|-----------|
| Rust/WASM learning curve dla zespołu | Średnie | Solo lub mały zespół — owner uczy się iteracyjnie; dobre typy chronią |
| Strategic map 720k hex performance | Niskie | Instanced WebGL + LOD, spatial index w Rust, Web Worker |
| Sector 1M hex battle tick 20Hz | Średnie | HPA* pathfinding, spatial partition, entity pooling, profiling |
| Multiplayer desync | Niskie | Deterministic sim w Rust, fixed-point, replay testing |
| WASM bundle size (>5MB) | Średnie | wasm-opt, code splitting, lazy load sector logic |
| AI nation quality | Wysokie | Behavioral trees + telemetry-driven tuning, playtesting |
| Balance (drony OP?) | Wysokie | Telemetry + beta feedback loops, config-as-data = łatwy hotfix |
| Player retention (too complex?) | Średnie | Progressive complexity, tutorial, etap 1-4 = simple |

---

## Następne kroki

1. **Teraz**: Walidacja konceptu (landing page, survey, Discord)
2. **Tydzień 1-2**: Monorepo setup (Cargo workspace + pnpm), docker compose, `game-core` skeleton z hex math + WASM bindings
3. **Tydzień 3-4**: Map pipeline (Python GDAL), Postgres seed, pierwsze tiles w Pixi
4. **Tydzień 5-8**: Strategic renderer (LOD + wrap), sector scaffold, Axum routes
5. **Tydzień 6-7 (równolegle)**: WebLLM PoC + polski benchmark (Qwen 3 0.6B / 1.7B vs Gemma 3 1B) na `/lab/llm`
6. **Miesiąc 3+**: FoW, sector zoom transition, base building

**Kamienie milowe**:
- **#1**: Strategic mapa w browserze z heksami + zoom (2 tyg)
- **#2**: Sector map, zoom transition strategic↔sector (4 tyg)
- **#3**: Budynki w sektorze + jednostki poruszają się (8 tyg)
- **#4**: Pierwsza RTS bitwa w sektorze (14 tyg)
- **#5**: Dwóch graczy gra przeciwko sobie (20 tyg)

---

## Assety graficzne

### Podejście: placeholder → iteracja → finalne

```
FAZA 1 — PLACEHOLDER (etapy 1-3):
  Colored hexagony + NATO military symbols + tekst
  → Czołg = prostokąt z "ARM"
  → Piechota = kwadrat z "INF"
  → Zero art potrzebnego

FAZA 2 — BASIC SPRITES (etap 4-5):
  Proste pixel art sprites (32-64px), 8 kierunków
  Źródła: OpenGameArt, Kenney, itch.io, AI-generated

FAZA 3 — POLISHED (etap 8+):
  Dedykowany art lub AI-generated + cleanup
  Shadery: water, night/day cycle, weather, radiation glow
```

### Darmowe / tanie źródła

```
MAPY/TERRAIN: OpenGameArt, Kenney.nl, Natural Earth, NASA SRTM
UNIT SPRITES: OpenGameArt, Kenney, itch.io ($5-20)
UI: Kenney UI Pack, game-icons.net
EFEKTY: OpenGameArt particles, itch.io VFX
AI SPRITES: Midjourney/SDXL/Flux → cleanup → spritesheet
SOUND: Freesound.org, incompetech.com (Kevin MacLeod)
MUZYKA AI: Suno/Udio (ambient strategy tracks)
```

### Dual mode: Pretty + NATO

```
PRETTY MODE (default):  pixel art, animacje, efekty, ładny terrain
NATO MODE (hardcore):   czyste symbole NATO APP-6, max info density

Gracz przełącza hotkey. NATO mode = feature dla maniaków wojskowych,
nie placeholder. Hardcore players wolą czytelność nad ładność.
```

### Art Direction

Styl: 32-64px pixel art + nowoczesne shadery. Inspiracje: Northgard (mapa), Into the Breach (units), Company of Heroes (efekty).

Pipeline AI-assisted: SDXL base sprite → cleanup ręczny → rotacje 8 kierunków → spritesheet → PixiJS particle system dla effects.

---

## Checkpoint: stan projektu

**Data**: 2026-04-17
**Status**: Plan zaktualizowany — Rust+WASM+PixiJS+Next16, dwupoziomowa mapa, rozmiary 1200x600 / 512-1024
**Dokument**: `/home/konrad-sedkowski/code/grand-strategy-game-plan.md`
**Następny krok**: Etap 1.1 — monorepo setup (Cargo workspace + pnpm) + docker compose + game-core skeleton. Repo: `~/code/grand-strategy/`.

**Równolegle w etapie 1**: 1.6 WebLLM PoC — weryfikacja małych LLM (Qwen 3 1.7B primary, Gemma 3 1B fallback) pod polskim flavor text, deterministyczna logika AI zostaje w `game-core`.
