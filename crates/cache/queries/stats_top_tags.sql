SELECT json_extract(j.value, '$.name') AS name, COUNT(DISTINCT v.work_id) AS count
FROM files f
INNER JOIN versions v ON v.content_hash = f.content_hash, json_each(v.tags) j
WHERE f.target = ? AND json_extract(j.value, '$.kind') = ?
GROUP BY name
ORDER BY count DESC
LIMIT ?
