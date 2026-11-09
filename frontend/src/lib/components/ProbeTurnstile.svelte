<script lang="ts">
    import { LoaderCircle } from "@lucide/svelte";
    import { onMount } from "svelte";
    import { loadTurnstile } from "$lib/turnstile";

    const {
        onToken,
        verified = false,
    }: {
        onToken: (token: string) => void;
        verified?: boolean;
    } = $props();
    let container: HTMLDivElement;
    let failed = $state(false);
    let tokenReady = $state(false);
    let retry = $state(0);
    let mounted = $state(false);

    onMount(() => {
        mounted = true;
    });

    $effect(() => {
        if (!mounted) return;
        void retry;
        let cancelled = false;
        let dispose: (() => void) | undefined;
        failed = false;
        tokenReady = false;
        loadTurnstile()
            .then((api) => {
                if (cancelled) return;
                const id = api.render(container, {
                    sitekey: "0x4AAAAAAF1P31qgYnY9u87Z",
                    action: "probe",
                    theme: "dark",
                    appearance: "interaction-only",
                    callback: (token) => {
                        if (!cancelled) {
                            tokenReady = true;
                            onToken(token);
                        }
                    },
                    "expired-callback": () => {
                        if (!cancelled) {
                            tokenReady = false;
                            onToken("");
                        }
                    },
                    "error-callback": () => {
                        if (!cancelled) {
                            failed = true;
                            onToken("");
                        }
                    },
                });
                dispose = () => api.remove(id);
            })
            .catch(() => {
                if (!cancelled) {
                    failed = true;
                    onToken("");
                }
            });
        return () => {
            cancelled = true;
            dispose?.();
        };
    });
</script>

<div class={verified && tokenReady && !failed ? "contents" : "space-y-3"}>
    {#if !failed && !(verified && tokenReady)}
        <div
            role="status"
            aria-live="polite"
            class="rounded-lg border border-neutral-800 bg-neutral-900/20 p-8 text-center text-sm text-neutral-400"
        >
            <LoaderCircle
                class="mx-auto mb-3 animate-spin text-primary"
                size={28}
            />
            Проверка безопасности…
        </div>
    {/if}
    <div bind:this={container}></div>
    {#if failed}
        <p role="alert" class="text-sm text-red-400">
            Не удалось пройти проверку Turnstile. Попробуйте перезагрузить
            страницу.
        </p>
        <button
            type="button"
            class="text-sm text-neutral-200 underline"
            onclick={() => {
                onToken("");
                retry += 1;
            }}
        >
            Повторить проверку
        </button>
    {/if}
</div>
