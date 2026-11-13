export type Probe = {
	id: number;
	name: string;
	region: string | null;
	asn: string | null;
	provider: string | null;
	hidden: boolean;
	disable_traceroutes: boolean;
	cdn_unblocked: boolean;
	last_connected_at: string | null;
	online: boolean;
	version: string | null;
	bundle_type: string | null;
	dpi_hop_v4: number | null;
	dpi_hop_v6: number | null;
};
export type Hop = {
	ttl: number;
	address: string | null;
	reverse_names: string[];
	outcome: string;
	tcp_diagnostics?: TcpDiagnostics;
};
export type TcpPacket = {
	observed_ms: number;
	source: string;
	destination: string;
	ttl: number | null;
	ip_id: number | null;
	sequence: number;
	acknowledgment: number;
	window: number;
	flags: string[];
	timestamp: number | null;
	timestamp_echo: number | null;
	options_hex: string;
	payload_bytes: number;
};
export type TcpDiagnostics = {
	packets: TcpPacket[];
	capture_error: string | null;
	connect_error: string | null;
	send_error: string | null;
	truncated: boolean;
	client_hello_sent_ms: number | null;
	junk_sent_ms: number | null;
	payload_sent?: boolean;
	peer_closed_before_payload?: boolean;
};
export type CalibrationDebug = {
	method: string;
	dpi_hop: number | null;
	fallback_reason?: string;
	failure_reason?: string;
	control_sni?: string;
	last_clear_ttl?: number;
	first_reset_ttl?: number;
	control_witness_ttl?: number;
	attempts?: {
		ttl: number;
		control: boolean;
		outcome: string;
		router: string | null;
		error?: string;
		tcp_diagnostics?: TcpDiagnostics;
	}[];
};
export type DpiHop = {
	ttl: number;
	src: string | null;
	outcome: string;
	tcp_diagnostics?: TcpDiagnostics;
};
export type CommandType =
	| "resubscribe_tasks"
	| "traceroute"
	| "remeasure_dpi_hop"
	| "sni_traceroute";
export type CommandResult =
	| { type: "traceroute"; target: string; hops: Hop[] }
	| { type: "resubscribe_tasks"; requested: boolean }
	| {
			type: "remeasure_dpi_hop";
			dpi_hop_v4: number | null;
			dpi_hop_v6: number | null;
			dpi_hops_v4: DpiHop[];
			dpi_hops_v6: DpiHop[];
			calibration_v4?: CalibrationDebug;
			calibration_v6?: CalibrationDebug;
	  }
	| {
			type: "sni_traceroute";
			host: string;
			sni: string;
			target: string;
			dpi_hop: number | null;
			hops: DpiHop[];
			calibration?: CalibrationDebug;
	  }
	| { type: "error"; message: string };
export type ProbeForm = Pick<
	Probe,
	| "name"
	| "region"
	| "asn"
	| "provider"
	| "hidden"
	| "disable_traceroutes"
	| "cdn_unblocked"
>;

export type ApiToken = {
	credits: number;
	id: string;
	name: string;
	created_at: string;
	expires_at: string | null;
	revoked_at: string | null;
};
export type IssuedApiToken = Pick<ApiToken, "id" | "name" | "expires_at"> & {
	token: string;
};
