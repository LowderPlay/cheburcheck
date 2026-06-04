import { getContext, setContext } from "svelte";

export interface Heading {
	id: string;
	title: string;
}

class KbState {
	headings = $state<Heading[]>([]);

	registerHeading(heading: Heading) {
		if (this.headings.some((h) => h.id === heading.id)) return;
		this.headings.push(heading);
	}

	reset() {
		this.headings = [];
	}
}

const KB_CONTEXT_KEY = Symbol("kb_context");

export function setKbContext() {
	return setContext(KB_CONTEXT_KEY, new KbState());
}

export function getKbContext() {
	return getContext<KbState>(KB_CONTEXT_KEY);
}
