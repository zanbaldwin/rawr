-- Storage totals for one target: decompressed content bytes across the
-- distinct versions present, and on-disk (compressed) bytes.
SELECT
    (SELECT COALESCE(SUM(content_size), 0) FROM versions v
      WHERE EXISTS (SELECT 1 FROM files f WHERE f.content_hash = v.content_hash AND f.target = ?1)) AS content_size,
    (SELECT COALESCE(SUM(file_size), 0) FROM files WHERE target = ?1)                               AS file_size
