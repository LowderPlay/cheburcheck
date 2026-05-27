type ApiNetworkRecord = {
	provider: string;
	cidr: string;
	region?: string | null;
};

type ApiAsnInfo = {
	asn: number;
	prefixes: string[];
	blocked_prefixes: string[];
};

type ApiWhitelist = {
	domain?: string | null;
	rank?: number | null;
	last_ok?: string | null;
};

type ApiCheckResponse = {
	id?: string | null;
	target: string;
	target_type: string;
	blocked: boolean;
	rkn_domain?: string | null;
	ips: string[];
	blocked_subnets: string[];
	cdn_providers: Record<string, ApiNetworkRecord[]>;
	geo: {
		asn?: string | null;
		country_code?: string | null;
		organisation?: string | null;
		city_geo_name_id?: number | null;
		location: string;
	};
	asn_info?: ApiAsnInfo | null;
	whitelist?: ApiWhitelist | null;
	subnet_size?: string | null;
};

export type CheckResult = {
	id?: string | null;
	targetType: string;
	target: string;
	found: boolean;
	blocked: boolean;
	whitelist?: { lastOk?: string | null } | null;
	domain?: string | null;
	ips: string[];
	subnetSize?: string | null;
	geo: { organisation?: string | null; location: string; asn?: string | null };
	providers: { name: string; networks: ApiNetworkRecord[] }[];
	blockedSubnets: string[];
	asnInfo: { prefixes: string[]; blockedPrefixes: string[] } | null;
};

export class CheckRequestError extends Error {
	constructor(
		message: string,
		readonly status: number,
	) {
		super(message);
		this.name = "CheckRequestError";
	}
}

const sortPrefixes = (prefixes: string[]) => [
	...prefixes.filter((prefix) => !prefix.includes(":")),
	...prefixes.filter((prefix) => prefix.includes(":")),
];

export async function fetchCheck(target: string): Promise<CheckResult> {
	const response = await fetch(
		`/api/v1/check?target=${encodeURIComponent(target)}`,
		{
			headers: {
				Accept: "application/json",
			},
		},
	);

	if (!response.ok) {
		throw new CheckRequestError(response.statusText, response.status);
	}

	const data = (await response.json()) as ApiCheckResponse;

	return {
		id: data.id,
		targetType: data.target_type,
		target: data.target,
		found: data.blocked,
		blocked: data.blocked,
		whitelist: data.whitelist
			? {
					lastOk: data.whitelist.last_ok,
				}
			: null,
		domain: data.rkn_domain,
		ips: data.ips,
		subnetSize: data.subnet_size,
		geo: data.geo,
		providers: Object.entries(data.cdn_providers).map(([name, networks]) => ({
			name,
			networks,
		})),
		blockedSubnets: data.blocked_subnets,
		asnInfo: data.asn_info
			? {
					prefixes: sortPrefixes(data.asn_info.prefixes),
					blockedPrefixes: sortPrefixes(data.asn_info.blocked_prefixes),
				}
			: null,
	};
}
