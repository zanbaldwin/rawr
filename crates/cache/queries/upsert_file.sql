INSERT INTO files (target, path, compression, file_size, file_hash, content_hash, discovered_at)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (target, path) DO UPDATE SET
    compression = excluded.compression,
    file_size = excluded.file_size,
    file_hash = excluded.file_hash,
    content_hash = excluded.content_hash,
    discovered_at = excluded.discovered_at
WHERE file_hash != excluded.file_hash;
