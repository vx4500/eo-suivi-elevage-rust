-- Saisie vocale : trace de chaque énoncé dicté depuis l'application mobile.
-- La ligne est créée à l'analyse, avant toute écriture métier : rien n'est
-- enregistré dans l'élevage tant que l'éleveur n'a pas validé la relecture.
-- L'audio n'est conservé que le temps de comprendre les échecs de
-- transcription (voir `vocal::RETENTION_AUDIO_JOURS`), le texte reste.
CREATE TABLE IF NOT EXISTS saisievocale (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    utilisateur TEXT,
    audio BLOB,
    audio_mime TEXT,
    audio_purge_at TEXT,
    texte_brut TEXT,
    analyse_json TEXT,
    statut TEXT NOT NULL DEFAULT 'analysee',
    truie_id INTEGER REFERENCES truie(id) ON DELETE SET NULL,
    perte_id INTEGER REFERENCES perteporcelet(id) ON DELETE SET NULL,
    valide_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_saisievocale_created ON saisievocale(created_at);
CREATE INDEX IF NOT EXISTS idx_saisievocale_statut ON saisievocale(statut,created_at);
