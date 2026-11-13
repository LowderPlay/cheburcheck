import type { Probe, ProbeForm } from "$lib/api/adminModels";

export const emptyForm = (): ProbeForm => ({
	name: "",
	region: "",
	asn: "",
	provider: "",
	hidden: false,
	disable_traceroutes: false,
	cdn_unblocked: false,
});

export function probeForm(probe: Probe): ProbeForm {
	const {
		name,
		region,
		asn,
		provider,
		hidden,
		disable_traceroutes,
		cdn_unblocked,
	} = probe;
	return {
		name,
		region,
		asn,
		provider,
		hidden,
		disable_traceroutes,
		cdn_unblocked,
	};
}

export const outcomeLabel: Record<string, string> = {
	icmp_time_exceeded: "Промежуточный узел",
	rst: "TCP RST",
	connected: "Подключено",
	tcp_closed: "TCP закрыт",
	tcp_acknowledged: "TCP подтверждён",
	timeout: "Нет ответа",
};
