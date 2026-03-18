-- Covering index for stats queries: target filter + join key + file_size aggregation
-- Avoids table lookups on files for every stats query
CREATE INDEX IF NOT EXISTS idx_files_target_content_hash_filesize
ON files(target, content_hash, file_size);
