-- ─── Battlefield Instances ──────────────────────────────────────────
-- Each strategic hex can have a battlefield instance, lazy-created
-- when a player first zooms in. Persists afterwards.

CREATE TABLE battlefield_instances (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategic_q     INT NOT NULL,
    strategic_r     INT NOT NULL,
    size            SMALLINT NOT NULL DEFAULT 64 CHECK (size IN (32, 64, 128)),
    seed            BIGINT NOT NULL,
    status          VARCHAR(16) NOT NULL DEFAULT 'CALM' CHECK (status IN ('CALM', 'ACTIVE')),
    macro_terrain   VARCHAR(32) NOT NULL DEFAULT 'Plains',
    macro_elevation INT NOT NULL DEFAULT 0,
    terrain_blob    BYTEA,                  -- serialized battlefield cells (optional, for large instances)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (strategic_q, strategic_r)
);

CREATE INDEX idx_battlefield_status ON battlefield_instances (status);

-- ─── Buildings ──────────────────────────────────────────────────────
-- Buildings placed on battlefield hexes.

CREATE TABLE buildings (
    id              BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    battlefield_id  UUID NOT NULL REFERENCES battlefield_instances(id) ON DELETE CASCADE,
    building_type   VARCHAR(32) NOT NULL,
    hex_q           INT NOT NULL,
    hex_r           INT NOT NULL,
    level           SMALLINT NOT NULL DEFAULT 1,
    hp              INT NOT NULL,
    max_hp          INT NOT NULL,
    nation_id       UUID REFERENCES nations(id),
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (battlefield_id, hex_q, hex_r)
);

CREATE INDEX idx_buildings_battlefield ON buildings (battlefield_id);
CREATE INDEX idx_buildings_nation ON buildings (nation_id) WHERE nation_id IS NOT NULL;

-- ─── Construction Queue ────────────────────────────────────────────
-- Active construction orders for battlefield instances.

CREATE TABLE construction_orders (
    id              BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    battlefield_id  UUID NOT NULL REFERENCES battlefield_instances(id) ON DELETE CASCADE,
    building_type   VARCHAR(32) NOT NULL,
    hex_q           INT NOT NULL,
    hex_r           INT NOT NULL,
    nation_id       UUID REFERENCES nations(id),
    status          VARCHAR(16) NOT NULL DEFAULT 'QUEUED' CHECK (status IN ('QUEUED', 'IN_PROGRESS', 'COMPLETED', 'CANCELLED')),
    progress        FLOAT NOT NULL DEFAULT 0.0,
    total_time      FLOAT NOT NULL,
    elapsed         FLOAT NOT NULL DEFAULT 0.0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_construction_battlefield ON construction_orders (battlefield_id);
CREATE INDEX idx_construction_status ON construction_orders (status);

-- ─── Resource Deposits ─────────────────────────────────────────────
-- Deposits on battlefield hexes (generated with the battlefield).

CREATE TABLE resource_deposits (
    id              BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    battlefield_id  UUID NOT NULL REFERENCES battlefield_instances(id) ON DELETE CASCADE,
    deposit_type    VARCHAR(16) NOT NULL CHECK (deposit_type IN ('Metals', 'Oil', 'Farmland', 'Timber')),
    hex_q           INT NOT NULL,
    hex_r           INT NOT NULL,
    richness        FLOAT NOT NULL DEFAULT 1.0,
    remaining       FLOAT,                  -- NULL = infinite
    UNIQUE (battlefield_id, hex_q, hex_r)
);

CREATE INDEX idx_deposits_battlefield ON resource_deposits (battlefield_id);
