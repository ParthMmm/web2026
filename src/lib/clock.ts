/** Seconds as a playback time: "7:15", or "1:22:15" past the hour. */
export const clock = (seconds: number): string => {
	const h = Math.floor(seconds / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	const s = String(Math.floor(seconds % 60)).padStart(2, "0");
	return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
};
