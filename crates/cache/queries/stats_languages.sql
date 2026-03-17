SELECT v.lang, COUNT(DISTINCT v.work_id) AS count
FROM files f
INNER JOIN versions v ON v.content_hash = f.content_hash
WHERE f.target = ?
GROUP BY v.lang
ORDER BY count DESC
