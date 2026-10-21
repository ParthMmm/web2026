/* A Swiss event two rounds in, with eight players from Dune, Andor, and The
   Lord of the Rings. The numbers follow the rules Möte implements: 3 points a
   win, OMW% with the 33% floor as the first tiebreaker, and round 3 pairing
   equal points without rematches. */

export const players = ["Paul", "Chani", "Cassian", "Éowyn", "Luthen", "Aragorn", "Stilgar", "Bix"] as const;
export type Player = (typeof players)[number];

/** Rounds 1 and 2, as winner and loser. */
const played: readonly (readonly [Player, Player])[] = [
	["Paul", "Luthen"],
	["Chani", "Stilgar"],
	["Cassian", "Aragorn"],
	["Éowyn", "Bix"],
	["Paul", "Éowyn"],
	["Chani", "Cassian"],
	["Luthen", "Stilgar"],
	["Aragorn", "Bix"],
];

/** Round 3: equal points, no rematches. */
export const pairings: readonly (readonly [Player, Player])[] = [
	["Paul", "Chani"],
	["Cassian", "Éowyn"],
	["Luthen", "Aragorn"],
	["Stilgar", "Bix"],
];

export type Standing = { name: Player; points: number; omw: number };

const MATCH_WIN_FLOOR = 1 / 3;

/** `winners` holds the reported winner of each round 3 table, by index. */
export const standings = (winners: readonly (Player | undefined)[]): Standing[] => {
	const matches = [...played];
	for (const [table, winner] of winners.entries()) {
		const pair = pairings[table];
		if (winner && pair) {
			matches.push([winner, pair[0] === winner ? pair[1] : pair[0]]);
		}
	}

	const record = (name: Player) => {
		const wins = matches.filter(([w]) => w === name).length;
		const total = matches.filter(([w, l]) => w === name || l === name).length;
		return { total, wins };
	};
	const winRate = (name: Player) => {
		const { total, wins } = record(name);
		return Math.max(wins / total, MATCH_WIN_FLOOR);
	};

	return players
		.map((name) => {
			const opponents = matches.flatMap(([w, l]) => {
				if (w === name) {
					return [l];
				}
				return l === name ? [w] : [];
			});
			const omw = opponents.reduce((sum, o) => sum + winRate(o), 0) / opponents.length;
			return { name, omw, points: record(name).wins * 3 };
		})
		.toSorted((a, b) => b.points - a.points || b.omw - a.omw || a.name.localeCompare(b.name));
};

export const percent = (value: number): string => `${Math.round(value * 100)}%`;

/** A familiar-style 5×5 pixel sprite, mirrored, seeded from the name. */
export const sprite = (name: string): boolean[] => {
	let seed = [...name].reduce((hash, char) => (hash * 31 + char.charCodeAt(0)) >>> 0, 7);
	const half: boolean[] = [];
	for (let i = 0; i < 15; i += 1) {
		seed = (seed * 1_103_515_245 + 12_345) >>> 0;
		half.push((seed >>> 16) % 3 !== 0);
	}
	return Array.from({ length: 25 }, (_, i) => {
		const row = Math.floor(i / 5);
		const col = i % 5;
		return half[row * 3 + (col < 3 ? col : 4 - col)] ?? false;
	});
};
