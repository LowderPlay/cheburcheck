<script lang="ts">
    import KbCode from "./KbCode.svelte";

    const {
        method = "GET",
        url,
        headers = {},
        requestBody = "",
        responseBody = "",
        responseStatus = "200 OK",
        responseType = "application/json",
    }: {
        method?: "GET" | "POST" | "PUT" | "DELETE";
        url: string;
        headers?: Record<string, string>;
        requestBody?: string;
        responseBody?: string;
        responseStatus?: string;
        responseType?: string;
    } = $props();

    const id = $props.id();
    const tabs = [
        { key: "request", label: "Тело запроса" },
        { key: "response", label: "Тело ответа" },
    ] as const;
    let activeTab = $state<"request" | "response">("response");

    function navigateTabs(event: KeyboardEvent, index: number) {
        let next: number;
        switch (event.key) {
            case "ArrowRight":
                next = (index + 1) % tabs.length;
                break;
            case "ArrowLeft":
                next = (index + tabs.length - 1) % tabs.length;
                break;
            case "Home":
                next = 0;
                break;
            case "End":
                next = tabs.length - 1;
                break;
            default:
                return;
        }
        event.preventDefault();
        activeTab = tabs[next].key;
        (event.currentTarget as HTMLButtonElement).parentElement
            ?.querySelectorAll<HTMLButtonElement>("[role=tab]")
            [next]?.focus();
    }
</script>

<section
    aria-label={`${method} ${url}`}
    class="my-8 min-w-0 overflow-hidden rounded-xl border border-neutral-800 bg-neutral-900/30"
>
    <div class="flex items-start gap-3 border-b border-neutral-800 px-4 py-4">
        <span
            class="rounded border border-emerald-800/70 bg-emerald-950/40 px-2 py-1 font-mono text-xs font-bold text-emerald-300"
            >{method}</span
        >
        <span
            class="min-w-0 break-all pt-0.5 font-mono text-sm leading-6 text-neutral-200"
            >{url}</span
        >
    </div>
    {#if Object.keys(headers).length > 0}
        <div class="space-y-2 border-b border-neutral-800 px-4 py-3">
            <div class="text-xs text-neutral-500">Заголовки запроса</div>
            {#each Object.entries(headers) as [name, value]}
                <div class="break-all font-mono text-xs leading-5">
                    <span class="text-neutral-500">{name}:&nbsp;</span><span
                        class="text-neutral-300">{value}</span
                    >
                </div>
            {/each}
        </div>
    {/if}
    <div
        role="tablist"
        aria-label="Запрос и ответ"
        class="flex border-b border-neutral-800 px-4"
    >
        {#each tabs as tab, index}
            <button
                type="button"
                role="tab"
                id={`${id}-${tab.key}-tab`}
                aria-selected={activeTab === tab.key}
                aria-controls={`${id}-${tab.key}-panel`}
                tabindex={activeTab === tab.key ? 0 : -1}
                class={`cursor-pointer border-b-2 px-3 py-3 text-sm transition-colors focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-neutral-300 ${activeTab === tab.key ? "border-neutral-200 text-neutral-100" : "border-transparent text-neutral-500 hover:text-neutral-300"}`}
                onclick={() => {
                    activeTab = tab.key;
                }}
                onkeydown={(event) => navigateTabs(event, index)}
            >
                {tab.label}
            </button>
        {/each}
    </div>
    {#each tabs as tab}
        {@const body = tab.key === "request" ? requestBody : responseBody}
        <div
            role="tabpanel"
            id={`${id}-${tab.key}-panel`}
            aria-labelledby={`${id}-${tab.key}-tab`}
            hidden={activeTab !== tab.key}
            tabindex="0"
            class="px-4 py-4 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-neutral-300 [&_pre]:my-0 [&_pre]:max-h-[32rem] [&_pre]:shadow-none"
        >
            {#if tab.key === "response"}
                <div
                    class="mb-3 flex flex-wrap gap-x-4 gap-y-1 font-mono text-xs"
                >
                    <span class="text-emerald-300">{responseStatus}</span><span
                        class="text-neutral-500">{responseType}</span
                    >
                </div>
            {/if}
            {#if body}
                <KbCode copyable>{body}</KbCode>
            {:else}
                <div class="py-4 text-sm text-neutral-500">
                    {tab.key === "request"
                        ? "Тело запроса не требуется. Параметры передаются в URL и заголовках."
                        : "Тело ответа отсутствует."}
                </div>
            {/if}
        </div>
    {/each}
    w
</section>
