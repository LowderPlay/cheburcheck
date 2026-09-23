import { type GeoPermissibleObjects, geoConicEqualArea, geoPath } from "d3-geo";
import boundaries from "$lib/data/russiaRegions.json";

type PolygonGeometry = { type: "Polygon"; coordinates: number[][][] };
type MultiPolygonGeometry = {
	type: "MultiPolygon";
	coordinates: number[][][][];
};

// D3's spherical clipping expects the opposite ring winding from GeoJSON.
// Keep the source GeoJSON intact and reverse only the rings passed to d3-geo.
const projectedFeatures = boundaries.features.map((feature) => {
	const geometry = feature.geometry as PolygonGeometry | MultiPolygonGeometry;
	return {
		...feature,
		geometry: {
			...geometry,
			coordinates:
				geometry.type === "Polygon"
					? geometry.coordinates.map((ring) => ring.toReversed())
					: geometry.coordinates.map((polygon) =>
							polygon.map((ring) => ring.toReversed()),
						),
		},
	} as GeoPermissibleObjects;
});
const collection = {
	type: "FeatureCollection",
	features: projectedFeatures,
} as GeoPermissibleObjects;
const projection = geoConicEqualArea()
	.parallels([40, 60])
	.rotate([-105, 0])
	.fitExtent(
		[
			[8, 8],
			[512, 242],
		],
		collection,
	);
const path = geoPath(projection);

export const russiaSubjects = boundaries.features.map((feature, index) => ({
	id: feature.properties.id,
	name: feature.properties.name,
	path: path(projectedFeatures[index]) ?? "",
}));

const names: Record<string, string> = {
	"RU-ALT": "Алтайский край",
	"RU-MO": "Республика Мордовия",
	"RU-TUL": "Тульская область",
	"RU-KGN": "Курганская область",
	"RU-IN": "Республика Ингушетия",
	"RU-KHM": "Ханты-Мансийский автономный округ — Югра",
	"RU-KIR": "Кировская область",
	"RU-KO": "Республика Коми",
	"RU-KOS": "Костромская область",
	"RU-KYA": "Красноярский край",
	"RU-ZAB": "Забайкальский край",
	"RU-SVE": "Свердловская область",
	"RU-VGG": "Волгоградская область",
	"RU-IRK": "Иркутская область",
	"RU-PER": "Пермский край",
	"RU-PSK": "Псковская область",
	"RU-ROS": "Ростовская область",
	"RU-RYA": "Рязанская область",
	"RU-AD": "Республика Адыгея",
	"RU-SAM": "Самарская область",
	"RU-KK": "Республика Хакасия",
	"RU-TAM": "Тамбовская область",
	"RU-TA": "Республика Татарстан",
	"RU-TOM": "Томская область",
	"RU-NIZ": "Нижегородская область",
	"RU-KR": "Республика Карелия",
	"RU-ARK": "Архангельская область",
	"RU-AST": "Астраханская область",
	"RU-BEL": "Белгородская область",
	"RU-BRY": "Брянская область",
	"RU-BU": "Республика Бурятия",
	"RU-CE": "Чеченская Республика",
	"RU-CHE": "Челябинская область",
	"RU-CU": "Чувашская Республика",
	"RU-TYU": "Тюменская область",
	"RU-SE": "Республика Северная Осетия — Алания",
	"RU-PNZ": "Пензенская область",
	"RU-AMU": "Амурская область",
	"RU-KB": "Кабардино-Балкарская Республика",
	"RU-KDA": "Краснодарский край",
	"RU-KRS": "Курская область",
	"RU-LEN": "Ленинградская область",
	"RU-ME": "Республика Марий Эл",
	"RU-MOW": "Москва",
	"RU-MOS": "Московская область",
	"RU-MUR": "Мурманская область",
	"RU-NEN": "Ненецкий автономный округ",
	"RU-NGR": "Новгородская область",
	"RU-NVS": "Новосибирская область",
	"RU-OMS": "Омская область",
	"RU-ORL": "Орловская область",
	"RU-SPE": "Санкт-Петербург",
	"RU-SAK": "Сахалинская область",
	"RU-SA": "Республика Саха (Якутия)",
	"RU-SAR": "Саратовская область",
	"RU-SMO": "Смоленская область",
	"RU-STA": "Ставропольский край",
	"RU-TY": "Республика Тыва",
	"RU-TVE": "Тверская область",
	"RU-UD": "Удмуртская Республика",
	"RU-KLU": "Калужская область",
	"RU-LIP": "Липецкая область",
	"RU-MAG": "Магаданская область",
	"RU-ULY": "Ульяновская область",
	"RU-VLA": "Владимирская область",
	"RU-VLG": "Вологодская область",
	"RU-YAR": "Ярославская область",
	"RU-VOR": "Воронежская область",
	"RU-YAN": "Ямало-Ненецкий автономный округ",
	"RU-AL": "Республика Алтай",
	"RU-IVA": "Ивановская область",
	"RU-YEV": "Еврейская автономная область",
	"RU-KL": "Республика Калмыкия",
	"RU-KAM": "Камчатский край",
	"RU-KC": "Карачаево-Черкесская Республика",
	"RU-KEM": "Кемеровская область",
	"RU-KHA": "Хабаровский край",
	"RU-CHU": "Чукотский автономный округ",
	"RU-DA": "Республика Дагестан",
	"RU-KGD": "Калининградская область",
	"RU-ORE": "Оренбургская область",
	"RU-PRI": "Приморский край",
	"RU-BA": "Республика Башкортостан",
	"RU-CRI": "Республика Крым",
	"RU-SEV": "Севастополь",
	"RU-DON": "Донецкая Народная Республика",
	"RU-LUG": "Луганская Народная Республика",
	"RU-ZAP": "Запорожская область",
	"RU-KHE": "Херсонская область",
};

const aliases: Record<string, string> = {
	удмуртия: "RU-UD",
	чувашия: "RU-CU",
	якутия: "RU-SA",
	"республика саха": "RU-SA",
	хмао: "RU-KHM",
	"хмао югра": "RU-KHM",
	югра: "RU-KHM",
	янао: "RU-YAN",
	"кемеровская область кузбасс": "RU-KEM",
	кузбасс: "RU-KEM",
	"республика северная осетия алания": "RU-SE",
	"северная осетия": "RU-SE",
	"марий эл": "RU-ME",
	"санкт петербург": "RU-SPE",
	спб: "RU-SPE",
	"город москва": "RU-MOW",
	"г москва": "RU-MOW",
	крым: "RU-CRI",
	"город севастополь": "RU-SEV",
	"г севастополь": "RU-SEV",
	днр: "RU-DON",
	"донецкая область": "RU-DON",
	лнр: "RU-LUG",
	"луганская область": "RU-LUG",
};

function normalize(value: string) {
	return value
		.toLowerCase()
		.replaceAll("ё", "е")
		.replace(/[^a-zа-я0-9]+/g, " ")
		.trim()
		.replace(/\s+/g, " ");
}

const byName = new Map<string, string>();
for (const subject of russiaSubjects) {
	const russianName = names[subject.id];
	if (russianName) byName.set(normalize(russianName), subject.id);
	byName.set(normalize(subject.name), subject.id);
	byName.set(normalize(subject.id), subject.id);
}
for (const [name, id] of Object.entries(aliases))
	if (id) byName.set(normalize(name), id);

export function regionCode(name: string | null | undefined): string | null {
	if (!name) return null;
	return byName.get(normalize(name)) ?? null;
}

export function subjectName(id: string): string {
	return (
		names[id] ?? russiaSubjects.find((subject) => subject.id === id)?.name ?? id
	);
}
