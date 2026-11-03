# KHC-052: Waypoint Plaza, the hub's own map

| | |
|---|---|
| Gate | burba asked for an inspired hub map world for multiplayer (after KHC-041) |
| Owner | burba |
| Prerequisites | KHC-041 |
| Source | None: built in code from plain shapes and vertex colours. The room around it reads the player's install as usual |
| Wire | none |

## What was done

- `room/hub_map.rs`: [hub choice, KHC-052] Waypoint Plaza, an original town square at dusk, the hub's default map (`--hub-map mirage` keeps the Mirage Arena):
  - a round stone floor 2000 units across in checkered rings, with lighter paths to the doors and pads;
  - a fountain at the centre (a basin with a rim to jump onto and water inside), a spire, and a crystal that turns and bobs over it;
  - four duel pads on the diagonals: standing on one says Looking for match to the hub, and two players on pads are matched after 3 s (KHC-048);
  - twelve lamps, eight planters to jump onto, a wall all round;
  - two doors: Practice (north) starts the training room with a scripted rival, Tutorial (south) starts the Test KH3 mode (Attack at the door);
  - a town's skyline beyond the wall with lit windows, and a sky from violet overhead to orange at the horizon.
- Collision is the map's own triangles (floors and walls) in place of the room's; the follow camera collides with its walls. Sora is held 1900 from the centre.
- Colours are picked by eye and turned into the mesh's linear vertex colours. The crystal stands still with Reduce motion on.

## Results

| Run | Frames | 1% low |
|---|---|---|
| `bots-16` on the plaza, first colours | 60 | 53 |
| `flow`: two clients, relay, hub, duel and back | not measured | not measured |

The capture shows the plaza, pads, lamps, planters, the doors' frames, the skyline and avatars with their tags. The first colours were pale; made linear, the `flow` run captured them darker, at dusk.

## Evidence

Private, in `%USERPROFILE%\kh2-kh3-ref\runs\brief6\bots-16\`.

## Checks

| Check | Group | Verdict |
|---|---|---|
| Spawns and pads stand on floor at the floor's height, clear of the fountain, inside the wall; doors are reachable | Agent | PASS |
| The map's collision has floors and walls; positions and colours match | Agent | PASS |
| A capture of the plaza with avatars | Local | PASS (first colours) |
| The linear colours; the pads in use | Local | PASS in `flow` (KHC-048) |
| The doors | Local | NOT RUN |
| The owner likes it | Owner | NOT RUN |

## Uncertainty

Nobody has walked it with a pad: jump heights onto the rim (70) and planters (60) were chosen from KH2's jump, not tried.

## Verdict

PARTIAL. Built and seen in one capture; walking it, the doors and the final colours not seen.
