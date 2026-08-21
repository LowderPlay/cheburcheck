export type ProbeHostResult = {
	host_id: string;
	host: string;
	probe_evidence:
		| {
				type: "ConnectionError" | "ClientHello" | "Good";
		  }
		| {
				type: "DataTimeout";
				bytes: number;
		  };
};

export type DnsObservation = {
	provider: string;
	protocol: "Udp" | "Tcp" | "Doh" | "Dot";
	suspected_spoofing: boolean;
	metadata: {
		response_codes: string[];
		ipv4_count: number;
		ipv6_count: number;
	};
	outcome:
		| { type: "Answer"; addresses: string[] }
		| { type: "NoRecords" }
		| { type: "Error"; message: string };
};

export type ProbeVerdict =
	| "uncertain"
	| "dns_spoofing"
	| "sni_block"
	| "tspu_block"
	| "whitelist"
	| "ok";

export type DisplayProbeVerdict = ProbeVerdict | "cdn_block";
export type ResolvedProbeVerdict = Exclude<DisplayProbeVerdict, "uncertain">;

export type ProbeResult = {
	job_id: string;
	probe_id: string;
	region?: string | null;
	provider?: string | null;
	asn?: string | null;
	verdicts: ProbeVerdict[];
	host_results: ProbeHostResult[] | null;
	target_hop: number | null;
	dpi_hop: number | null;
	dns: {
		spoofing_detected: boolean;
		suspicious_provider_count: number;
		verdict_threshold: number;
		samples_per_protocol: number;
		observations: DnsObservation[];
	} | null;
};

export function displayProbeVerdicts(
	probe: ProbeResult,
	isStaticBlocked: boolean,
): DisplayProbeVerdict[] {
	return probe.verdicts.map((verdict) =>
		isStaticBlocked && probe.host_results?.length !== 0 && verdict === "ok"
			? "cdn_block"
			: verdict,
	);
}

const verdictPriority: DisplayProbeVerdict[] = [
	"tspu_block",
	"sni_block",
	"dns_spoofing",
	"whitelist",
	"cdn_block",
	"ok",
	"uncertain",
];

export function selectProbeVerdict(
	probes: ProbeResult[],
	isStaticBlocked: boolean,
): ResolvedProbeVerdict | null {
	if (probes.length === 0) return null;

	const votes = new Map<DisplayProbeVerdict, number>();
	for (const probe of probes) {
		for (const verdict of new Set(
			displayProbeVerdicts(probe, isStaticBlocked),
		)) {
			votes.set(verdict, (votes.get(verdict) ?? 0) + 1);
		}
	}

	let winner: DisplayProbeVerdict | null = null;
	let winningVotes = 0;
	for (const verdict of verdictPriority) {
		const count = votes.get(verdict) ?? 0;
		if (count > winningVotes) {
			winner = verdict;
			winningVotes = count;
		}
	}

	return winner === "uncertain" ? null : winner;
}

export type ProbeStatus = {
	id: string;
	target: string;
	online_probes: number;
	response_count: number;
	status: "started" | "progress" | "done" | "error";
};

export function startProbeSSE(
	id: string,
	onResult: (result: ProbeResult) => void,
	onStatus: (status: Partial<ProbeStatus>) => void,
) {
	const eventSource = new EventSource(`/api/v1/probe/${id}`);

	eventSource.addEventListener("started", (event) => {
		const data = JSON.parse(event.data);
		onStatus({
			id: data.id,
			target: data.target,
			online_probes: data.online_probes,
			status: "started",
			response_count: 0,
		});
	});

	eventSource.addEventListener("result", (event) => {
		const data = JSON.parse(event.data) as ProbeResult;
		onResult(data);
		onStatus({ status: "progress" });
	});

	eventSource.addEventListener("done", (event) => {
		const data = JSON.parse(event.data);
		onStatus({
			status: "done",
			response_count: data.response_count,
			online_probes: data.online_probes,
		});
		eventSource.close();
	});

	eventSource.onerror = (error) => {
		console.error("Probe SSE error:", error);
		onStatus({ status: "error" });
		eventSource.close();
	};

	return () => eventSource.close();
}
