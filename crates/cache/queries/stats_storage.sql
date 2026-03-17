SELECT COALESCE(SUM(f.file_size), 0) AS total_file_size
FROM files f
WHERE f.target = ?
