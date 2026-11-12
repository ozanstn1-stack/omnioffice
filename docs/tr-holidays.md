# Turkish religious holidays after 2029

`src/office/calc/tr-holidays.ts` lists the first day of Ramazan Bayramı and Kurban Bayramı.
2020–2029 are copied from Diyanet İşleri Başkanlığı's published calendar. 2030–2050 are
**calculated**, because Diyanet does not publish that far ahead in a form we can copy.

## Rule

Since 2017 Diyanet starts each Hijri month by the rule adopted at the 2016 International Hijri
Calendar Union Congress (Istanbul):

- A month begins the next day when, anywhere on Earth before 00:00 UTC, the Moon is at least
  **5°** above the horizon at sunset and at least **8°** from the Sun (elongation).
- A sighting in the Americas after 00:00 UTC also counts, provided the conjunction happened
  before dawn (fajr) in New Zealand.

Ramazan Bayramı is 1 Şevval (3 days); Kurban Bayramı is 10 Zilhicce (4 days). The eve of each
feast is a half day.

## Calculation and check

The generator below evaluates the rule on a world grid of observers (latitude −60°…60°) at each
sunset after every new moon, using [astronomy-engine](https://github.com/cosinekitty/astronomy)
2.1.19 for the Sun and Moon positions. It reproduces **all twenty** 2020–2029 feast dates in the
Diyanet table. The 2030–2050 output was identical with a 2° and a 1° grid and with topocentric
or geocentric lunar altitude. One date depends on the Americas exception: Ramazan Bayramı
2043 (4 September) starts because the Moon first qualifies in the Americas after 00:00 UTC; the
exception is evaluated on the earliest qualifying sunset in the Americas, not the earliest one
worldwide (an earlier draft did the latter and gave 5 September on a 1° grid).

Calculated dates can still differ from what Diyanet finally publishes, so the deadline
calculator keeps warning for years after 2029 (`TR_HOLIDAY_YEARS` lists only the copied years).
When Diyanet publishes a year, copy its dates into the table and add the year to
`TR_HOLIDAY_YEARS`.

Run it outside the repository:

```sh
mkdir hijri && cd hijri && npm init -y && npm i astronomy-engine@2.1.19
STEP=2 END=2051 node gen.mjs   # prints [["ramazan","2030-02-04"], ["kurban", ...], ...]
```

```js
import * as A from "astronomy-engine";
const STEP = Number(process.env.STEP ?? 3);
const TOPO = process.env.TOPO !== "0";
function obsGrid(latMin, latMax, lonMin, lonMax) {
  const out = [];
  for (let lat = latMin; lat <= latMax; lat += STEP) for (let lon = lonMin; lon <= lonMax; lon += STEP) out.push(new A.Observer(lat, lon, 0));
  return out;
}
const WORLD = obsGrid(-60, 60, -180, 177);
function qualifies(obs, t) {
  const elong = A.AngleFromSun(A.Body.Moon, t);
  if (elong < 8) return false;
  const eq = A.Equator(A.Body.Moon, t, obs, true, TOPO);
  const hor = A.Horizon(t, obs, eq.ra, eq.dec, null);
  return hor.altitude >= 5;
}
function earliest(conj, observers) {
  let best = null;
  for (const obs of observers) {
    let t = conj;
    for (let i = 0; i < 2; i++) {
      const s = A.SearchRiseSet(A.Body.Sun, obs, -1, t, 2);
      if (!s) break;
      if (qualifies(obs, s)) { if (!best || s.ut < best.t.ut) best = { t: s, obs }; break; }
      t = s.AddDays(0.01);
    }
  }
  return best;
}
const AMERICAS = WORLD.filter((o) => o.longitude <= -30 && o.longitude >= -170);
const nz = new A.Observer(-41.3, 174.8, 0);
const utcDay = (date) => Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate());
function monthStart(conj) {
  // General rule: the first sunset anywhere that qualifies; the month starts the next UTC day.
  const b = earliest(conj, WORLD);
  let start = utcDay(b.t.date) + 86400e3;
  // Americas exception: a qualifying sunset there after 00:00 UTC, still on the previous local
  // day, counts when the conjunction came before dawn (fajr) in New Zealand.
  const am = earliest(conj, AMERICAS);
  if (am) {
    const local = utcDay(new Date(am.t.date.getTime() + (am.obs.longitude / 15) * 3600e3));
    if (local < utcDay(am.t.date)) {
      const fajr = A.SearchAltitude(A.Body.Sun, nz, +1, A.MakeTime(new Date(local + 10 * 3600e3)), 1, -18);
      if (fajr && conj.ut < fajr.ut) start = Math.min(start, local + 86400e3);
    }
  }
  return new Date(start);
}
// anchor: the new moon that starts Ramazan 1450 (1 Ramazan = 2029-01-16)
let t = A.MakeTime(new Date(Date.UTC(2019, 0, 1)));
const moons = [];
while (t.date.getUTCFullYear() < Number(process.env.END ?? 2052)) { const m = A.SearchMoonPhase(0, t, 40); moons.push(m); t = m.AddDays(5); }
const anchor = moons.findIndex((m) => m.date > new Date(Date.UTC(2029, 1, 5))) - 1;
const fmt = (d) => d.toISOString().slice(0, 10);
const res = [];
for (let i = 0; i < moons.length; i++) {
  const month = (((i - anchor) % 12) + 12) % 12; // 0 = Ramazan, 1 = Şevval, 3 = Zilhicce
  if (month !== 1 && month !== 3) continue;
  if (moons[i].date.getUTCFullYear() < Number(process.env.FROM ?? 0)) continue;
  const s = monthStart(moons[i]);
  const day = month === 1 ? s : new Date(s.getTime() + 9 * 86400e3);
  res.push([month === 1 ? "ramazan" : "kurban", fmt(day)]);
}
console.log(JSON.stringify(res));
```
