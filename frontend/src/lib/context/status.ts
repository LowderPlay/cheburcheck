import type { CreateQueryResult } from "@tanstack/svelte-query";
import { createContext } from "svelte";
import type { StatusMetrics } from "$lib/api/status";

type StatusContext = CreateQueryResult<StatusMetrics, Error>;

export const [getStatusContext, setStatusContext] =
	createContext<StatusContext>();
