# Границы субъектов РФ

`russiaRegions.json` создан из [geoBoundaries RUS ADM1](https://www.geoboundaries.org/api/current/gbOpen/RUS/ADM1/) и [UKR ADM1](https://www.geoboundaries.org/api/current/gbOpen/UKR/ADM1/) (наборы 2017 года). 

Исходные границы: OpenStreetMap и Wambacher; © OpenStreetMap contributors. Метаданные источника указывают лицензию ODbL 1.0; geoBoundaries распространяет gbOpen на условиях CC BY 4.0.

Файл сокращён для карты: координаты округлены и линии упрощены. Для воспроизведения скачайте `simplifiedGeometryGeoJSON` по обеим ссылкам API выше и выполните:

```sh
python3 frontend/scripts/build-russia-regions.py /path/to/RUS_simplified.geojson /path/to/UKR_simplified.geojson
```
