SELECT f.path
FROM files f
WHERE f.target = ?
ORDER BY f.path
