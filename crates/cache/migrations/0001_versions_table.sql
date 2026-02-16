-- Versions table: stores extracted metadata for each unique HTML content
-- Primary key is content_hash (BLAKE3 hash of decompressed HTML)
CREATE TABLE IF NOT EXISTS versions (
    content_hash TEXT PRIMARY KEY NOT NULL,
    content_crc32 INT NOT NULL,  -- CRC32 for path templates (8 chars)
    work_id INT NOT NULL,
    content_size INT NOT NULL,
    title TEXT NOT NULL,
    authors TEXT NOT NULL, -- JSON Array of Objects
    fandoms TEXT NOT NULL, -- JSON Array
    series TEXT NOT NULL, -- JSON Array of Objects
    chapters_written INT NOT NULL,
    chapters_total INT,
    complete INT GENERATED ALWAYS AS (chapters_total IS NOT NULL AND chapters_written = chapters_total) VIRTUAL,
    words INT NOT NULL,
    summary TEXT, -- Markdown
    rating TEXT, -- Single Character
    warnings TEXT NOT NULL, -- JSON Array
    lang TEXT NOT NULL, -- Word
    published_on INT NOT NULL,
    last_modified INT NOT NULL,
    tags TEXT NOT NULL, -- JSON Array of Objects
    extracted_at INT NOT NULL  -- Unix timestamp
);

-- Index for finding all versions of a work
CREATE INDEX IF NOT EXISTS idx_versions_work_id ON versions(work_id);
