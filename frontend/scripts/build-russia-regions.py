"""Create lightweight ADM1 GeoJSON for the frontend from geoBoundaries.

Sources: geoBoundaries gbOpen RUS ADM1 and UKR ADM1, revision 9469f09.
Source attribution: geoBoundaries, William & Mary; OpenStreetMap contributors.
The source metadata lists ODbL 1.0; geoBoundaries gbOpen is distributed as CC BY 4.0.
Usage: python3 frontend/scripts/build-russia-regions.py /path/to/RUS_simplified.geojson /path/to/UKR_simplified.geojson
"""

import json
import sys
from pathlib import Path

RUSSIA_SOURCE = Path(sys.argv[1])
UKRAINE_SOURCE = Path(sys.argv[2])
TARGET = Path(__file__).resolve().parents[1] / "src/lib/data/russiaRegions.json"
TOLERANCE = 0.45
SOURCE_REGION_IDS = {
    "UA-43": ("RU-CRI", "Республика Крым"),
    "UA-40": ("RU-SEV", "Севастополь"),
    "UA-14": ("RU-DON", "Донецкая Народная Республика"),
    "UA-09": ("RU-LUG", "Луганская Народная Республика"),
    "UA-23": ("RU-ZAP", "Запорожская область"),
    "UA-65": ("RU-KHE", "Херсонская область"),
}


def point(coordinates):
    lon, lat = coordinates
    return (round(lon + (360 if lon < 0 else 0), 3), round(lat, 3))


def distance_squared(point, start, end):
    x, y = point
    x1, y1 = start
    x2, y2 = end
    dx, dy = x2 - x1, y2 - y1
    if dx == dy == 0:
        return (x - x1) ** 2 + (y - y1) ** 2
    t = max(0, min(1, ((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy)))
    return (x - x1 - t * dx) ** 2 + (y - y1 - t * dy) ** 2


def pixels(p):
    return ((p[0] - 19) * 4.4, (84 - p[1]) * 5.3)


def rdp(points):
    if len(points) <= 2:
        return points
    start, end = pixels(points[0]), pixels(points[-1])
    distances = [distance_squared(pixels(p), start, end) for p in points[1:-1]]
    index = max(range(len(distances)), key=distances.__getitem__) if distances else 0
    if distances and distances[index] > TOLERANCE * TOLERANCE:
        split = index + 1
        return rdp(points[:split + 1])[:-1] + rdp(points[split:])
    return [points[0], points[-1]]


def ring(coordinates):
    points = []
    for raw in coordinates:
        p = point(raw)
        if not points or p != points[-1]:
            points.append(p)
    if points[0] == points[-1]:
        points.pop()
    if len(points) < 3:
        return None
    anchor = min(range(len(points)), key=lambda i: points[i][0])
    points = points[anchor:] + points[:anchor]
    farthest = max(range(1, len(points)), key=lambda i: distance_squared(pixels(points[i]), pixels(points[0]), pixels(points[-1])))
    result = rdp(points[:farthest + 1])[:-1] + rdp(points[farthest:] + [points[0]])
    if len(result) < 4:
        return None
    return result


def polygon(coordinates):
    rings = [result for raw in coordinates if (result := ring(raw))]
    return rings if rings else None


def geometry(source):
    if source["type"] == "Polygon":
        result = polygon(source["coordinates"])
        return {"type": "Polygon", "coordinates": result} if result else None
    polygons = [result for raw in source["coordinates"] if (result := polygon(raw))]
    return {"type": "MultiPolygon", "coordinates": polygons} if polygons else None


source = json.loads(RUSSIA_SOURCE.read_text())
ukraine = json.loads(UKRAINE_SOURCE.read_text())
features = []
for feature in [*source["features"], *(item for item in ukraine["features"] if item["properties"]["shapeISO"] in SOURCE_REGION_IDS)]:
    shape = geometry(feature["geometry"])
    if shape:
        code = feature["properties"]["shapeISO"]
        region_id, name = SOURCE_REGION_IDS.get(code, (code, feature["properties"]["shapeName"]))
        features.append({
            "type": "Feature",
            "properties": {
                "id": region_id,
                "name": name,
            },
            "geometry": shape,
        })
TARGET.write_text(json.dumps({"type": "FeatureCollection", "features": features}, ensure_ascii=False, separators=(",", ":")))
print(f"{len(features)} regions, {TARGET.stat().st_size} bytes -> {TARGET}")
