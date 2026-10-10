WITH toDate(now(), $2) AS today_day
SELECT toString(days.day) AS date, toInt64(ifNull(reports.count, 0)) AS count
FROM (
    SELECT today_day - 13 + toInt32(number) AS day
    FROM numbers(14)
) AS days
LEFT JOIN (
    SELECT toDate(date, $2) AS day, uniqExact(source_ip) AS count
    FROM human_reports
    WHERE date >= toDateTime(today_day - 13, $2)
      AND date < toDateTime(today_day + 1, $2)
      AND works = false
      AND query_lower = lowerUTF8($1)
    GROUP BY day
) AS reports ON reports.day = days.day
ORDER BY days.day
