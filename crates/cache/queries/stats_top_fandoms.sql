SELECT j.value AS name, COUNT(DISTINCT v.work_id) AS count
FROM files f
INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.fandoms) j
WHERE f.target = ?
GROUP BY j.value
ORDER BY count DESC
LIMIT ?
