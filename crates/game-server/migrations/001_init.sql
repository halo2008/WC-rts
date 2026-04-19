-- Enable PostGIS extension
CREATE EXTENSION IF NOT EXISTS postgis;

-- ─── Nations ────────────────────────────────────────────────────────
CREATE TABLE nations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            VARCHAR(128) NOT NULL UNIQUE,
    code            VARCHAR(8) NOT NULL UNIQUE,
    government_type VARCHAR(32) NOT NULL DEFAULT 'democracy',
    tier            SMALLINT NOT NULL DEFAULT 3 CHECK (tier BETWEEN 1 AND 4),
    color           VARCHAR(7) NOT NULL DEFAULT '#888888',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Hex Map ────────────────────────────────────────────────────────
CREATE TABLE hex_map (
    q       INT NOT NULL,
    r       INT NOT NULL,
    terrain VARCHAR(32) NOT NULL DEFAULT 'Plains',
    elevation INT NOT NULL DEFAULT 0,
    resource  VARCHAR(32),
    nation_id UUID REFERENCES nations(id),
    infrastructure_level SMALLINT NOT NULL DEFAULT 0,
    geom    GEOMETRY(POINT, 4326) GENERATED ALWAYS AS (
        ST_SetSRID(ST_MakePoint(q::float, r::float), 4326)
    ) STORED,
    PRIMARY KEY (q, r)
);

CREATE INDEX idx_hex_map_geom ON hex_map USING GIST (geom);
CREATE INDEX idx_hex_map_nation ON hex_map (nation_id) WHERE nation_id IS NOT NULL;

-- ─── Territories ────────────────────────────────────────────────────
CREATE TABLE territories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    nation_id   UUID NOT NULL REFERENCES nations(id) ON DELETE CASCADE,
    hex_q       INT NOT NULL,
    hex_r       INT NOT NULL,
    claimed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hex_q, hex_r)
);

CREATE INDEX idx_territories_nation ON territories (nation_id);
