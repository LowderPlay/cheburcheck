import type { QueryClient } from "@tanstack/svelte-query";

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

export type ProbeResult = {
	job_id: string;
	probe_id: string;
	region?: string | null;
	provider?: string | null;
	asn?: string | null;
	verdict: "uncertain" | "sni_block" | "whitelist" | "ok";
	host_results: ProbeHostResult[];
};

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
