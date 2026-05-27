<script lang="ts">
import { TriangleAlert } from "@lucide/svelte";

const { status, reason }: { status: number; reason: string } = $props();

const error = $derived.by(() => {
	switch (status) {
		case 500:
			return "Внутренняя ошибка сервера. Пожалуйста, попробуйте позже.";
		case 502:
			return "Сервер временно недоступен. Попробуйте обновить страницу.";
		case 503:
			return "Сервис временно недоступен. Ведутся технические работы.";
		case 504:
			return "Превышено время ожидания ответа от сервера. Попробуйте позже.";
		case 400:
			return "Неверный запрос. Проверьте правильность введенных данных.";
		case 404:
			return "Страница не найдена. Проверьте правильность адреса.";
		case 422:
			return "Невозможно обработать запрос. Проверьте формат данных.";
		case 429:
			return "Слишком много запросов. Пожалуйста, подождите немного.";
		default:
			return "Что-то пошло не так. Возможно в запросе есть ошибка?";
	}
});
</script>

<div class="flex items-center gap-4">
	<TriangleAlert size={48} class="text-red-500" aria-hidden="true" />
	<h2 class="text-4xl trackin text-red-500 uppercase">
		{status}
		{reason}
	</h2>
</div>
<p class="text-2xl text-neutral-500">
	{error}
</p>
