import type { CatalogSnapshotOutcome, LocalGamesListOutcome, SessionStatusOutcome } from "@contracts/generated/korrid"

// Turtles records and all failures from the real rpminiv2 responses captured
// after the recurring refusal, 2026-10-27. Unrelated healthy games are omitted.
// Source: /tmp/sp-proof/mini-capture-3/{cat3,local3,sess3}.json.

export const miniCatalog: CatalogSnapshotOutcome = {
  "_tag": "Ok",
  "payload": {
    "games": [
      {
        "id": "teenage-mutant-ninja-turtles-iv-turtles-in-time-usa",
        "title": "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA)",
        "host": "rpminiv2",
        "identity": {
          "kind": "hash",
          "value": "sha256:5b82cdd6f2da56f43680d6a5021faebe2e06036d30602c1a7917aa414cf8b5f4"
        },
        "source": {
          "devicePublicKey": "8cd28f8a874f1eae75483ddbeedaeb4260b535598f4f4d6a852330bf7ede8e53",
          "label": "rpminiv2",
          "isLocal": true
        },
        "supportsRunnerSelection": true,
        "playStats": {
          "lastPlayed": "2026-10-27T21:40:22.000Z",
          "playCount": 6,
          "totalPlaytimeSeconds": 2474.0
        }
      },
      {
        "id": "teenage-mutant-ninja-turtles-iv-turtles-in-time-usa",
        "title": "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA)",
        "host": "rg35xxsp",
        "identity": {
          "kind": "hash",
          "value": "sha256:5b82cdd6f2da56f43680d6a5021faebe2e06036d30602c1a7917aa414cf8b5f4"
        },
        "source": {
          "devicePublicKey": "306cdc659700fb5510147aad9849d3abef6eb6d966977dbc55e77e84aafbc8e0",
          "label": "rg35xxsp",
          "isLocal": false
        },
        "supportsRunnerSelection": true,
        "playStats": {
          "lastPlayed": "2026-10-27T19:41:14.000Z",
          "playCount": 50,
          "totalPlaytimeSeconds": 136946.0
        }
      },
      {
        "id": "teenage-mutant-ninja-turtles-iv-turtles-in-time-usa",
        "title": "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA)",
        "host": "odin2portal",
        "identity": {
          "kind": "hash",
          "value": "sha256:5b82cdd6f2da56f43680d6a5021faebe2e06036d30602c1a7917aa414cf8b5f4"
        },
        "source": {
          "devicePublicKey": "6544cbd41e84cb4c2c1bf4391643551e5b9f0c4d27c140046a85369bfa0475ab",
          "label": "odin2portal",
          "isLocal": false
        },
        "supportsRunnerSelection": true,
        "playStats": {
          "lastPlayed": "2026-10-27T21:40:25.000Z",
          "playCount": 31,
          "totalPlaytimeSeconds": 391.0
        }
      }
    ],
    "failures": [
      {
        "host": "rpminiv2",
        "code": "LocalRomMissing",
        "message": "release sha256:f03f0f998d8f67abc8801ebd7fee977bafaa7449cfa758727a67fec1e57e2097 location is missing"
      },
      {
        "host": "rpminiv2",
        "code": "LocalRomMissing",
        "message": "release sha256:fd794e7a3025c3fd1340d25301658bac8c829a65d5e5e47d2663b431f869531b location is missing"
      },
      {
        "host": "rg35xxpro",
        "code": "UpstreamUnreachable",
        "message": "upstream unreachable: peer operation timed out"
      }
    ]
  }
}

export const miniLocalGames: LocalGamesListOutcome = {
  "_tag": "Ok",
  "payload": {
    "games": [
      {
        "id": "teenage-mutant-ninja-turtles-iv-turtles-in-time-usa",
        "title": "Teenage Mutant Ninja Turtles IV - Turtles in Time (USA)",
        "system": "Super Nintendo",
        "identity": {
          "kind": "hash",
          "value": "sha256:5b82cdd6f2da56f43680d6a5021faebe2e06036d30602c1a7917aa414cf8b5f4"
        }
      }
    ],
    "failures": [
      {
        "code": "LocalRomMissing",
        "message": "release sha256:f03f0f998d8f67abc8801ebd7fee977bafaa7449cfa758727a67fec1e57e2097 location is missing"
      },
      {
        "code": "LocalRomMissing",
        "message": "release sha256:fd794e7a3025c3fd1340d25301658bac8c829a65d5e5e47d2663b431f869531b location is missing"
      }
    ]
  }
}

export const miniSession: SessionStatusOutcome = {
  "_tag": "Err",
  "payload": {
    "code": "NoActiveSession",
    "message": "no host launch is active"
  }
}
