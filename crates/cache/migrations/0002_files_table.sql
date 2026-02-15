-- Files table: tracks physical files across all storage backends
-- Primary key is (target, path) to allow same path in different targets
CREATE TABLE IF NOT EXISTS files (
    target TEXT NOT NULL,       -- Storage target name from config (e.g., 'local', 'tigris')
    path TEXT NOT NULL,         -- Relative path from target root
    compression TEXT NOT NULL,  -- 'none', 'gzip', 'bzip2', 'xz', 'zstd', 'brotli'
    file_size INT NOT NULL,     -- Size of compressed file in bytes
    file_hash TEXT NOT NULL,    -- BLAKE3 hash of compressed file
    content_hash TEXT NOT NULL, -- FK to versions.content_hash
    discovered_at INT NOT NULL, -- Unix timestamp when metadata was extracted
    PRIMARY KEY (target, path),
    FOREIGN KEY (content_hash) REFERENCES versions(content_hash) ON DELETE CASCADE
);

-- Index for finding files by content hash (e.g., finding duplicates across targets)
CREATE INDEX IF NOT EXISTS idx_files_content_hash ON files(content_hash);
-- Index for finding files by file hash (change detection during scan)
CREATE INDEX IF NOT EXISTS idx_files_file_hash ON files(file_hash);
-- Index for listing files in a specific target
CREATE INDEX IF NOT EXISTS idx_files_target ON files(target);
