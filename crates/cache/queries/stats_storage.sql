SELECT
    COALESCE(SUM(f.file_size), 0) AS total_file_size,
    COALESCE(SUM(v.content_size), 0) AS total_content_size
FROM files f
LEFT JOIN versions v ON f.content_hash = v.content_hash
WHERE f.target = ?
