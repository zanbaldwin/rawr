SELECT f.content_hash, COUNT(*) as count
FROM files f
GROUP BY f.target, f.content_hash
HAVING count > 1
ORDER BY count DESC
