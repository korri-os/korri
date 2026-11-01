(() => {
  var __create = Object.create;
  var __getProtoOf = Object.getPrototypeOf;
  var __defProp = Object.defineProperty;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  function __accessProp(key) {
    return this[key];
  }
  var __toESMCache_node;
  var __toESMCache_esm;
  var __toESM = (mod, isNodeMode, target) => {
    var canCache = mod != null && typeof mod === "object";
    if (canCache) {
      var cache = isNodeMode ? __toESMCache_node ??= new WeakMap : __toESMCache_esm ??= new WeakMap;
      var cached = cache.get(mod);
      if (cached)
        return cached;
    }
    target = mod != null ? __create(__getProtoOf(mod)) : {};
    const to = isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target;
    for (let key of __getOwnPropNames(mod))
      if (!__hasOwnProp.call(to, key))
        __defProp(to, key, {
          get: __accessProp.bind(mod, key),
          enumerable: true
        });
    if (canCache)
      cache.set(mod, to);
    return to;
  };
  var __commonJS = (cb, mod) => () => (mod || cb((mod = { exports: {} }).exports, mod), mod.exports);

  // src/vendor/qrcode/utils.js
  var require_utils = __commonJS((exports) => {
    var toSJISFunction;
    var CODEWORDS_COUNT = [
      0,
      26,
      44,
      70,
      100,
      134,
      172,
      196,
      242,
      292,
      346,
      404,
      466,
      532,
      581,
      655,
      733,
      815,
      901,
      991,
      1085,
      1156,
      1258,
      1364,
      1474,
      1588,
      1706,
      1828,
      1921,
      2051,
      2185,
      2323,
      2465,
      2611,
      2761,
      2876,
      3034,
      3196,
      3362,
      3532,
      3706
    ];
    exports.getSymbolSize = function getSymbolSize(version) {
      if (!version)
        throw new Error('"version" cannot be null or undefined');
      if (version < 1 || version > 40)
        throw new Error('"version" should be in range from 1 to 40');
      return version * 4 + 17;
    };
    exports.getSymbolTotalCodewords = function getSymbolTotalCodewords(version) {
      return CODEWORDS_COUNT[version];
    };
    exports.getBCHDigit = function(data) {
      let digit = 0;
      while (data !== 0) {
        digit++;
        data >>>= 1;
      }
      return digit;
    };
    exports.setToSJISFunction = function setToSJISFunction(f) {
      if (typeof f !== "function") {
        throw new Error('"toSJISFunc" is not a valid function.');
      }
      toSJISFunction = f;
    };
    exports.isKanjiModeEnabled = function() {
      return typeof toSJISFunction !== "undefined";
    };
    exports.toSJIS = function toSJIS(kanji) {
      return toSJISFunction(kanji);
    };
  });

  // src/vendor/qrcode/error-correction-level.js
  var require_error_correction_level = __commonJS((exports) => {
    exports.L = { bit: 1 };
    exports.M = { bit: 0 };
    exports.Q = { bit: 3 };
    exports.H = { bit: 2 };
    function fromString(string) {
      if (typeof string !== "string") {
        throw new Error("Param is not a string");
      }
      const lcStr = string.toLowerCase();
      switch (lcStr) {
        case "l":
        case "low":
          return exports.L;
        case "m":
        case "medium":
          return exports.M;
        case "q":
        case "quartile":
          return exports.Q;
        case "h":
        case "high":
          return exports.H;
        default:
          throw new Error("Unknown EC Level: " + string);
      }
    }
    exports.isValid = function isValid(level) {
      return level && typeof level.bit !== "undefined" && level.bit >= 0 && level.bit < 4;
    };
    exports.from = function from(value, defaultValue) {
      if (exports.isValid(value)) {
        return value;
      }
      try {
        return fromString(value);
      } catch (e) {
        return defaultValue;
      }
    };
  });

  // src/vendor/qrcode/bit-buffer.js
  var require_bit_buffer = __commonJS((exports, module) => {
    function BitBuffer() {
      this.buffer = [];
      this.length = 0;
    }
    BitBuffer.prototype = {
      get: function(index) {
        const bufIndex = Math.floor(index / 8);
        return (this.buffer[bufIndex] >>> 7 - index % 8 & 1) === 1;
      },
      put: function(num, length) {
        for (let i = 0;i < length; i++) {
          this.putBit((num >>> length - i - 1 & 1) === 1);
        }
      },
      getLengthInBits: function() {
        return this.length;
      },
      putBit: function(bit) {
        const bufIndex = Math.floor(this.length / 8);
        if (this.buffer.length <= bufIndex) {
          this.buffer.push(0);
        }
        if (bit) {
          this.buffer[bufIndex] |= 128 >>> this.length % 8;
        }
        this.length++;
      }
    };
    module.exports = BitBuffer;
  });

  // src/vendor/qrcode/bit-matrix.js
  var require_bit_matrix = __commonJS((exports, module) => {
    function BitMatrix(size) {
      if (!size || size < 1) {
        throw new Error("BitMatrix size must be defined and greater than 0");
      }
      this.size = size;
      this.data = new Uint8Array(size * size);
      this.reservedBit = new Uint8Array(size * size);
    }
    BitMatrix.prototype.set = function(row, col, value, reserved) {
      const index = row * this.size + col;
      this.data[index] = value;
      if (reserved)
        this.reservedBit[index] = true;
    };
    BitMatrix.prototype.get = function(row, col) {
      return this.data[row * this.size + col];
    };
    BitMatrix.prototype.xor = function(row, col, value) {
      this.data[row * this.size + col] ^= value;
    };
    BitMatrix.prototype.isReserved = function(row, col) {
      return this.reservedBit[row * this.size + col];
    };
    module.exports = BitMatrix;
  });

  // src/vendor/qrcode/alignment-pattern.js
  var require_alignment_pattern = __commonJS((exports) => {
    var getSymbolSize = require_utils().getSymbolSize;
    exports.getRowColCoords = function getRowColCoords(version) {
      if (version === 1)
        return [];
      const posCount = Math.floor(version / 7) + 2;
      const size = getSymbolSize(version);
      const intervals = size === 145 ? 26 : Math.ceil((size - 13) / (2 * posCount - 2)) * 2;
      const positions = [size - 7];
      for (let i = 1;i < posCount - 1; i++) {
        positions[i] = positions[i - 1] - intervals;
      }
      positions.push(6);
      return positions.reverse();
    };
    exports.getPositions = function getPositions(version) {
      const coords = [];
      const pos = exports.getRowColCoords(version);
      const posLength = pos.length;
      for (let i = 0;i < posLength; i++) {
        for (let j = 0;j < posLength; j++) {
          if (i === 0 && j === 0 || i === 0 && j === posLength - 1 || i === posLength - 1 && j === 0) {
            continue;
          }
          coords.push([pos[i], pos[j]]);
        }
      }
      return coords;
    };
  });

  // src/vendor/qrcode/finder-pattern.js
  var require_finder_pattern = __commonJS((exports) => {
    var getSymbolSize = require_utils().getSymbolSize;
    var FINDER_PATTERN_SIZE = 7;
    exports.getPositions = function getPositions(version) {
      const size = getSymbolSize(version);
      return [
        [0, 0],
        [size - FINDER_PATTERN_SIZE, 0],
        [0, size - FINDER_PATTERN_SIZE]
      ];
    };
  });

  // src/vendor/qrcode/mask-pattern.js
  var require_mask_pattern = __commonJS((exports) => {
    exports.Patterns = {
      PATTERN000: 0,
      PATTERN001: 1,
      PATTERN010: 2,
      PATTERN011: 3,
      PATTERN100: 4,
      PATTERN101: 5,
      PATTERN110: 6,
      PATTERN111: 7
    };
    var PenaltyScores = {
      N1: 3,
      N2: 3,
      N3: 40,
      N4: 10
    };
    exports.isValid = function isValid(mask) {
      return mask != null && mask !== "" && !isNaN(mask) && mask >= 0 && mask <= 7;
    };
    exports.from = function from(value) {
      return exports.isValid(value) ? parseInt(value, 10) : undefined;
    };
    exports.getPenaltyN1 = function getPenaltyN1(data) {
      const size = data.size;
      let points = 0;
      let sameCountCol = 0;
      let sameCountRow = 0;
      let lastCol = null;
      let lastRow = null;
      for (let row = 0;row < size; row++) {
        sameCountCol = sameCountRow = 0;
        lastCol = lastRow = null;
        for (let col = 0;col < size; col++) {
          let module2 = data.get(row, col);
          if (module2 === lastCol) {
            sameCountCol++;
          } else {
            if (sameCountCol >= 5)
              points += PenaltyScores.N1 + (sameCountCol - 5);
            lastCol = module2;
            sameCountCol = 1;
          }
          module2 = data.get(col, row);
          if (module2 === lastRow) {
            sameCountRow++;
          } else {
            if (sameCountRow >= 5)
              points += PenaltyScores.N1 + (sameCountRow - 5);
            lastRow = module2;
            sameCountRow = 1;
          }
        }
        if (sameCountCol >= 5)
          points += PenaltyScores.N1 + (sameCountCol - 5);
        if (sameCountRow >= 5)
          points += PenaltyScores.N1 + (sameCountRow - 5);
      }
      return points;
    };
    exports.getPenaltyN2 = function getPenaltyN2(data) {
      const size = data.size;
      let points = 0;
      for (let row = 0;row < size - 1; row++) {
        for (let col = 0;col < size - 1; col++) {
          const last = data.get(row, col) + data.get(row, col + 1) + data.get(row + 1, col) + data.get(row + 1, col + 1);
          if (last === 4 || last === 0)
            points++;
        }
      }
      return points * PenaltyScores.N2;
    };
    exports.getPenaltyN3 = function getPenaltyN3(data) {
      const size = data.size;
      let points = 0;
      let bitsCol = 0;
      let bitsRow = 0;
      for (let row = 0;row < size; row++) {
        bitsCol = bitsRow = 0;
        for (let col = 0;col < size; col++) {
          bitsCol = bitsCol << 1 & 2047 | data.get(row, col);
          if (col >= 10 && (bitsCol === 1488 || bitsCol === 93))
            points++;
          bitsRow = bitsRow << 1 & 2047 | data.get(col, row);
          if (col >= 10 && (bitsRow === 1488 || bitsRow === 93))
            points++;
        }
      }
      return points * PenaltyScores.N3;
    };
    exports.getPenaltyN4 = function getPenaltyN4(data) {
      let darkCount = 0;
      const modulesCount = data.data.length;
      for (let i = 0;i < modulesCount; i++)
        darkCount += data.data[i];
      const k = Math.abs(Math.ceil(darkCount * 100 / modulesCount / 5) - 10);
      return k * PenaltyScores.N4;
    };
    function getMaskAt(maskPattern, i, j) {
      switch (maskPattern) {
        case exports.Patterns.PATTERN000:
          return (i + j) % 2 === 0;
        case exports.Patterns.PATTERN001:
          return i % 2 === 0;
        case exports.Patterns.PATTERN010:
          return j % 3 === 0;
        case exports.Patterns.PATTERN011:
          return (i + j) % 3 === 0;
        case exports.Patterns.PATTERN100:
          return (Math.floor(i / 2) + Math.floor(j / 3)) % 2 === 0;
        case exports.Patterns.PATTERN101:
          return i * j % 2 + i * j % 3 === 0;
        case exports.Patterns.PATTERN110:
          return (i * j % 2 + i * j % 3) % 2 === 0;
        case exports.Patterns.PATTERN111:
          return (i * j % 3 + (i + j) % 2) % 2 === 0;
        default:
          throw new Error("bad maskPattern:" + maskPattern);
      }
    }
    exports.applyMask = function applyMask(pattern, data) {
      const size = data.size;
      for (let col = 0;col < size; col++) {
        for (let row = 0;row < size; row++) {
          if (data.isReserved(row, col))
            continue;
          data.xor(row, col, getMaskAt(pattern, row, col));
        }
      }
    };
    exports.getBestMask = function getBestMask(data, setupFormatFunc) {
      const numPatterns = Object.keys(exports.Patterns).length;
      let bestPattern = 0;
      let lowerPenalty = Infinity;
      for (let p = 0;p < numPatterns; p++) {
        setupFormatFunc(p);
        exports.applyMask(p, data);
        const penalty = exports.getPenaltyN1(data) + exports.getPenaltyN2(data) + exports.getPenaltyN3(data) + exports.getPenaltyN4(data);
        exports.applyMask(p, data);
        if (penalty < lowerPenalty) {
          lowerPenalty = penalty;
          bestPattern = p;
        }
      }
      return bestPattern;
    };
  });

  // src/vendor/qrcode/error-correction-code.js
  var require_error_correction_code = __commonJS((exports) => {
    var ECLevel = require_error_correction_level();
    var EC_BLOCKS_TABLE = [
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      1,
      2,
      2,
      1,
      2,
      2,
      4,
      1,
      2,
      4,
      4,
      2,
      4,
      4,
      4,
      2,
      4,
      6,
      5,
      2,
      4,
      6,
      6,
      2,
      5,
      8,
      8,
      4,
      5,
      8,
      8,
      4,
      5,
      8,
      11,
      4,
      8,
      10,
      11,
      4,
      9,
      12,
      16,
      4,
      9,
      16,
      16,
      6,
      10,
      12,
      18,
      6,
      10,
      17,
      16,
      6,
      11,
      16,
      19,
      6,
      13,
      18,
      21,
      7,
      14,
      21,
      25,
      8,
      16,
      20,
      25,
      8,
      17,
      23,
      25,
      9,
      17,
      23,
      34,
      9,
      18,
      25,
      30,
      10,
      20,
      27,
      32,
      12,
      21,
      29,
      35,
      12,
      23,
      34,
      37,
      12,
      25,
      34,
      40,
      13,
      26,
      35,
      42,
      14,
      28,
      38,
      45,
      15,
      29,
      40,
      48,
      16,
      31,
      43,
      51,
      17,
      33,
      45,
      54,
      18,
      35,
      48,
      57,
      19,
      37,
      51,
      60,
      19,
      38,
      53,
      63,
      20,
      40,
      56,
      66,
      21,
      43,
      59,
      70,
      22,
      45,
      62,
      74,
      24,
      47,
      65,
      77,
      25,
      49,
      68,
      81
    ];
    var EC_CODEWORDS_TABLE = [
      7,
      10,
      13,
      17,
      10,
      16,
      22,
      28,
      15,
      26,
      36,
      44,
      20,
      36,
      52,
      64,
      26,
      48,
      72,
      88,
      36,
      64,
      96,
      112,
      40,
      72,
      108,
      130,
      48,
      88,
      132,
      156,
      60,
      110,
      160,
      192,
      72,
      130,
      192,
      224,
      80,
      150,
      224,
      264,
      96,
      176,
      260,
      308,
      104,
      198,
      288,
      352,
      120,
      216,
      320,
      384,
      132,
      240,
      360,
      432,
      144,
      280,
      408,
      480,
      168,
      308,
      448,
      532,
      180,
      338,
      504,
      588,
      196,
      364,
      546,
      650,
      224,
      416,
      600,
      700,
      224,
      442,
      644,
      750,
      252,
      476,
      690,
      816,
      270,
      504,
      750,
      900,
      300,
      560,
      810,
      960,
      312,
      588,
      870,
      1050,
      336,
      644,
      952,
      1110,
      360,
      700,
      1020,
      1200,
      390,
      728,
      1050,
      1260,
      420,
      784,
      1140,
      1350,
      450,
      812,
      1200,
      1440,
      480,
      868,
      1290,
      1530,
      510,
      924,
      1350,
      1620,
      540,
      980,
      1440,
      1710,
      570,
      1036,
      1530,
      1800,
      570,
      1064,
      1590,
      1890,
      600,
      1120,
      1680,
      1980,
      630,
      1204,
      1770,
      2100,
      660,
      1260,
      1860,
      2220,
      720,
      1316,
      1950,
      2310,
      750,
      1372,
      2040,
      2430
    ];
    exports.getBlocksCount = function getBlocksCount(version, errorCorrectionLevel) {
      switch (errorCorrectionLevel) {
        case ECLevel.L:
          return EC_BLOCKS_TABLE[(version - 1) * 4 + 0];
        case ECLevel.M:
          return EC_BLOCKS_TABLE[(version - 1) * 4 + 1];
        case ECLevel.Q:
          return EC_BLOCKS_TABLE[(version - 1) * 4 + 2];
        case ECLevel.H:
          return EC_BLOCKS_TABLE[(version - 1) * 4 + 3];
        default:
          return;
      }
    };
    exports.getTotalCodewordsCount = function getTotalCodewordsCount(version, errorCorrectionLevel) {
      switch (errorCorrectionLevel) {
        case ECLevel.L:
          return EC_CODEWORDS_TABLE[(version - 1) * 4 + 0];
        case ECLevel.M:
          return EC_CODEWORDS_TABLE[(version - 1) * 4 + 1];
        case ECLevel.Q:
          return EC_CODEWORDS_TABLE[(version - 1) * 4 + 2];
        case ECLevel.H:
          return EC_CODEWORDS_TABLE[(version - 1) * 4 + 3];
        default:
          return;
      }
    };
  });

  // src/vendor/qrcode/galois-field.js
  var require_galois_field = __commonJS((exports) => {
    var EXP_TABLE = new Uint8Array(512);
    var LOG_TABLE = new Uint8Array(256);
    (function initTables() {
      let x = 1;
      for (let i = 0;i < 255; i++) {
        EXP_TABLE[i] = x;
        LOG_TABLE[x] = i;
        x <<= 1;
        if (x & 256) {
          x ^= 285;
        }
      }
      for (let i = 255;i < 512; i++) {
        EXP_TABLE[i] = EXP_TABLE[i - 255];
      }
    })();
    exports.log = function log(n) {
      if (n < 1)
        throw new Error("log(" + n + ")");
      return LOG_TABLE[n];
    };
    exports.exp = function exp(n) {
      return EXP_TABLE[n];
    };
    exports.mul = function mul(x, y) {
      if (x === 0 || y === 0)
        return 0;
      return EXP_TABLE[LOG_TABLE[x] + LOG_TABLE[y]];
    };
  });

  // src/vendor/qrcode/polynomial.js
  var require_polynomial = __commonJS((exports) => {
    var GF = require_galois_field();
    exports.mul = function mul(p1, p2) {
      const coeff = new Uint8Array(p1.length + p2.length - 1);
      for (let i = 0;i < p1.length; i++) {
        for (let j = 0;j < p2.length; j++) {
          coeff[i + j] ^= GF.mul(p1[i], p2[j]);
        }
      }
      return coeff;
    };
    exports.mod = function mod(divident, divisor) {
      let result = new Uint8Array(divident);
      while (result.length - divisor.length >= 0) {
        const coeff = result[0];
        for (let i = 0;i < divisor.length; i++) {
          result[i] ^= GF.mul(divisor[i], coeff);
        }
        let offset = 0;
        while (offset < result.length && result[offset] === 0)
          offset++;
        result = result.slice(offset);
      }
      return result;
    };
    exports.generateECPolynomial = function generateECPolynomial(degree) {
      let poly = new Uint8Array([1]);
      for (let i = 0;i < degree; i++) {
        poly = exports.mul(poly, new Uint8Array([1, GF.exp(i)]));
      }
      return poly;
    };
  });

  // src/vendor/qrcode/reed-solomon-encoder.js
  var require_reed_solomon_encoder = __commonJS((exports, module) => {
    var Polynomial = require_polynomial();
    function ReedSolomonEncoder(degree) {
      this.genPoly = undefined;
      this.degree = degree;
      if (this.degree)
        this.initialize(this.degree);
    }
    ReedSolomonEncoder.prototype.initialize = function initialize(degree) {
      this.degree = degree;
      this.genPoly = Polynomial.generateECPolynomial(this.degree);
    };
    ReedSolomonEncoder.prototype.encode = function encode(data) {
      if (!this.genPoly) {
        throw new Error("Encoder not initialized");
      }
      const paddedData = new Uint8Array(data.length + this.degree);
      paddedData.set(data);
      const remainder = Polynomial.mod(paddedData, this.genPoly);
      const start = this.degree - remainder.length;
      if (start > 0) {
        const buff = new Uint8Array(this.degree);
        buff.set(remainder, start);
        return buff;
      }
      return remainder;
    };
    module.exports = ReedSolomonEncoder;
  });

  // src/vendor/qrcode/version-check.js
  var require_version_check = __commonJS((exports) => {
    exports.isValid = function isValid(version) {
      return !isNaN(version) && version >= 1 && version <= 40;
    };
  });

  // src/vendor/qrcode/regex.js
  var require_regex = __commonJS((exports) => {
    var numeric = "[0-9]+";
    var alphanumeric = "[A-Z $%*+\\-./:]+";
    var kanji = "(?:[u3000-u303F]|[u3040-u309F]|[u30A0-u30FF]|" + "[uFF00-uFFEF]|[u4E00-u9FAF]|[u2605-u2606]|[u2190-u2195]|u203B|" + "[u2010u2015u2018u2019u2025u2026u201Cu201Du2225u2260]|" + "[u0391-u0451]|[u00A7u00A8u00B1u00B4u00D7u00F7])+";
    kanji = kanji.replace(/u/g, "\\u");
    var byte = "(?:(?![A-Z0-9 $%*+\\-./:]|" + kanji + `)(?:.|[\r
]))+`;
    exports.KANJI = new RegExp(kanji, "g");
    exports.BYTE_KANJI = new RegExp("[^A-Z0-9 $%*+\\-./:]+", "g");
    exports.BYTE = new RegExp(byte, "g");
    exports.NUMERIC = new RegExp(numeric, "g");
    exports.ALPHANUMERIC = new RegExp(alphanumeric, "g");
    var TEST_KANJI = new RegExp("^" + kanji + "$");
    var TEST_NUMERIC = new RegExp("^" + numeric + "$");
    var TEST_ALPHANUMERIC = new RegExp("^[A-Z0-9 $%*+\\-./:]+$");
    exports.testKanji = function testKanji(str) {
      return TEST_KANJI.test(str);
    };
    exports.testNumeric = function testNumeric(str) {
      return TEST_NUMERIC.test(str);
    };
    exports.testAlphanumeric = function testAlphanumeric(str) {
      return TEST_ALPHANUMERIC.test(str);
    };
  });

  // src/vendor/qrcode/mode.js
  var require_mode = __commonJS((exports) => {
    var VersionCheck = require_version_check();
    var Regex = require_regex();
    exports.NUMERIC = {
      id: "Numeric",
      bit: 1 << 0,
      ccBits: [10, 12, 14]
    };
    exports.ALPHANUMERIC = {
      id: "Alphanumeric",
      bit: 1 << 1,
      ccBits: [9, 11, 13]
    };
    exports.BYTE = {
      id: "Byte",
      bit: 1 << 2,
      ccBits: [8, 16, 16]
    };
    exports.KANJI = {
      id: "Kanji",
      bit: 1 << 3,
      ccBits: [8, 10, 12]
    };
    exports.MIXED = {
      bit: -1
    };
    exports.getCharCountIndicator = function getCharCountIndicator(mode, version) {
      if (!mode.ccBits)
        throw new Error("Invalid mode: " + mode);
      if (!VersionCheck.isValid(version)) {
        throw new Error("Invalid version: " + version);
      }
      if (version >= 1 && version < 10)
        return mode.ccBits[0];
      else if (version < 27)
        return mode.ccBits[1];
      return mode.ccBits[2];
    };
    exports.getBestModeForData = function getBestModeForData(dataStr) {
      if (Regex.testNumeric(dataStr))
        return exports.NUMERIC;
      else if (Regex.testAlphanumeric(dataStr))
        return exports.ALPHANUMERIC;
      else if (Regex.testKanji(dataStr))
        return exports.KANJI;
      else
        return exports.BYTE;
    };
    exports.toString = function toString(mode) {
      if (mode && mode.id)
        return mode.id;
      throw new Error("Invalid mode");
    };
    exports.isValid = function isValid(mode) {
      return mode && mode.bit && mode.ccBits;
    };
    function fromString(string) {
      if (typeof string !== "string") {
        throw new Error("Param is not a string");
      }
      const lcStr = string.toLowerCase();
      switch (lcStr) {
        case "numeric":
          return exports.NUMERIC;
        case "alphanumeric":
          return exports.ALPHANUMERIC;
        case "kanji":
          return exports.KANJI;
        case "byte":
          return exports.BYTE;
        default:
          throw new Error("Unknown mode: " + string);
      }
    }
    exports.from = function from(value, defaultValue) {
      if (exports.isValid(value)) {
        return value;
      }
      try {
        return fromString(value);
      } catch (e) {
        return defaultValue;
      }
    };
  });

  // src/vendor/qrcode/version.js
  var require_version = __commonJS((exports) => {
    var Utils = require_utils();
    var ECCode = require_error_correction_code();
    var ECLevel = require_error_correction_level();
    var Mode = require_mode();
    var VersionCheck = require_version_check();
    var G18 = 1 << 12 | 1 << 11 | 1 << 10 | 1 << 9 | 1 << 8 | 1 << 5 | 1 << 2 | 1 << 0;
    var G18_BCH = Utils.getBCHDigit(G18);
    function getBestVersionForDataLength(mode, length, errorCorrectionLevel) {
      for (let currentVersion = 1;currentVersion <= 40; currentVersion++) {
        if (length <= exports.getCapacity(currentVersion, errorCorrectionLevel, mode)) {
          return currentVersion;
        }
      }
      return;
    }
    function getReservedBitsCount(mode, version) {
      return Mode.getCharCountIndicator(mode, version) + 4;
    }
    function getTotalBitsFromDataArray(segments, version) {
      let totalBits = 0;
      segments.forEach(function(data) {
        const reservedBits = getReservedBitsCount(data.mode, version);
        totalBits += reservedBits + data.getBitsLength();
      });
      return totalBits;
    }
    function getBestVersionForMixedData(segments, errorCorrectionLevel) {
      for (let currentVersion = 1;currentVersion <= 40; currentVersion++) {
        const length = getTotalBitsFromDataArray(segments, currentVersion);
        if (length <= exports.getCapacity(currentVersion, errorCorrectionLevel, Mode.MIXED)) {
          return currentVersion;
        }
      }
      return;
    }
    exports.from = function from(value, defaultValue) {
      if (VersionCheck.isValid(value)) {
        return parseInt(value, 10);
      }
      return defaultValue;
    };
    exports.getCapacity = function getCapacity(version, errorCorrectionLevel, mode) {
      if (!VersionCheck.isValid(version)) {
        throw new Error("Invalid QR Code version");
      }
      if (typeof mode === "undefined")
        mode = Mode.BYTE;
      const totalCodewords = Utils.getSymbolTotalCodewords(version);
      const ecTotalCodewords = ECCode.getTotalCodewordsCount(version, errorCorrectionLevel);
      const dataTotalCodewordsBits = (totalCodewords - ecTotalCodewords) * 8;
      if (mode === Mode.MIXED)
        return dataTotalCodewordsBits;
      const usableBits = dataTotalCodewordsBits - getReservedBitsCount(mode, version);
      switch (mode) {
        case Mode.NUMERIC:
          return Math.floor(usableBits / 10 * 3);
        case Mode.ALPHANUMERIC:
          return Math.floor(usableBits / 11 * 2);
        case Mode.KANJI:
          return Math.floor(usableBits / 13);
        case Mode.BYTE:
        default:
          return Math.floor(usableBits / 8);
      }
    };
    exports.getBestVersionForData = function getBestVersionForData(data, errorCorrectionLevel) {
      let seg;
      const ecl = ECLevel.from(errorCorrectionLevel, ECLevel.M);
      if (Array.isArray(data)) {
        if (data.length > 1) {
          return getBestVersionForMixedData(data, ecl);
        }
        if (data.length === 0) {
          return 1;
        }
        seg = data[0];
      } else {
        seg = data;
      }
      return getBestVersionForDataLength(seg.mode, seg.getLength(), ecl);
    };
    exports.getEncodedBits = function getEncodedBits(version) {
      if (!VersionCheck.isValid(version) || version < 7) {
        throw new Error("Invalid QR Code version");
      }
      let d = version << 12;
      while (Utils.getBCHDigit(d) - G18_BCH >= 0) {
        d ^= G18 << Utils.getBCHDigit(d) - G18_BCH;
      }
      return version << 12 | d;
    };
  });

  // src/vendor/qrcode/format-info.js
  var require_format_info = __commonJS((exports) => {
    var Utils = require_utils();
    var G15 = 1 << 10 | 1 << 8 | 1 << 5 | 1 << 4 | 1 << 2 | 1 << 1 | 1 << 0;
    var G15_MASK = 1 << 14 | 1 << 12 | 1 << 10 | 1 << 4 | 1 << 1;
    var G15_BCH = Utils.getBCHDigit(G15);
    exports.getEncodedBits = function getEncodedBits(errorCorrectionLevel, mask) {
      const data = errorCorrectionLevel.bit << 3 | mask;
      let d = data << 10;
      while (Utils.getBCHDigit(d) - G15_BCH >= 0) {
        d ^= G15 << Utils.getBCHDigit(d) - G15_BCH;
      }
      return (data << 10 | d) ^ G15_MASK;
    };
  });

  // src/vendor/qrcode/numeric-data.js
  var require_numeric_data = __commonJS((exports, module) => {
    var Mode = require_mode();
    function NumericData(data) {
      this.mode = Mode.NUMERIC;
      this.data = data.toString();
    }
    NumericData.getBitsLength = function getBitsLength(length) {
      return 10 * Math.floor(length / 3) + (length % 3 ? length % 3 * 3 + 1 : 0);
    };
    NumericData.prototype.getLength = function getLength() {
      return this.data.length;
    };
    NumericData.prototype.getBitsLength = function getBitsLength() {
      return NumericData.getBitsLength(this.data.length);
    };
    NumericData.prototype.write = function write(bitBuffer) {
      let i, group, value;
      for (i = 0;i + 3 <= this.data.length; i += 3) {
        group = this.data.substr(i, 3);
        value = parseInt(group, 10);
        bitBuffer.put(value, 10);
      }
      const remainingNum = this.data.length - i;
      if (remainingNum > 0) {
        group = this.data.substr(i);
        value = parseInt(group, 10);
        bitBuffer.put(value, remainingNum * 3 + 1);
      }
    };
    module.exports = NumericData;
  });

  // src/vendor/qrcode/alphanumeric-data.js
  var require_alphanumeric_data = __commonJS((exports, module) => {
    var Mode = require_mode();
    var ALPHA_NUM_CHARS = [
      "0",
      "1",
      "2",
      "3",
      "4",
      "5",
      "6",
      "7",
      "8",
      "9",
      "A",
      "B",
      "C",
      "D",
      "E",
      "F",
      "G",
      "H",
      "I",
      "J",
      "K",
      "L",
      "M",
      "N",
      "O",
      "P",
      "Q",
      "R",
      "S",
      "T",
      "U",
      "V",
      "W",
      "X",
      "Y",
      "Z",
      " ",
      "$",
      "%",
      "*",
      "+",
      "-",
      ".",
      "/",
      ":"
    ];
    function AlphanumericData(data) {
      this.mode = Mode.ALPHANUMERIC;
      this.data = data;
    }
    AlphanumericData.getBitsLength = function getBitsLength(length) {
      return 11 * Math.floor(length / 2) + 6 * (length % 2);
    };
    AlphanumericData.prototype.getLength = function getLength() {
      return this.data.length;
    };
    AlphanumericData.prototype.getBitsLength = function getBitsLength() {
      return AlphanumericData.getBitsLength(this.data.length);
    };
    AlphanumericData.prototype.write = function write(bitBuffer) {
      let i;
      for (i = 0;i + 2 <= this.data.length; i += 2) {
        let value = ALPHA_NUM_CHARS.indexOf(this.data[i]) * 45;
        value += ALPHA_NUM_CHARS.indexOf(this.data[i + 1]);
        bitBuffer.put(value, 11);
      }
      if (this.data.length % 2) {
        bitBuffer.put(ALPHA_NUM_CHARS.indexOf(this.data[i]), 6);
      }
    };
    module.exports = AlphanumericData;
  });

  // src/vendor/qrcode/byte-data.js
  var require_byte_data = __commonJS((exports, module) => {
    var Mode = require_mode();
    function ByteData(data) {
      this.mode = Mode.BYTE;
      if (typeof data === "string") {
        this.data = new TextEncoder().encode(data);
      } else {
        this.data = new Uint8Array(data);
      }
    }
    ByteData.getBitsLength = function getBitsLength(length) {
      return length * 8;
    };
    ByteData.prototype.getLength = function getLength() {
      return this.data.length;
    };
    ByteData.prototype.getBitsLength = function getBitsLength() {
      return ByteData.getBitsLength(this.data.length);
    };
    ByteData.prototype.write = function(bitBuffer) {
      for (let i = 0, l = this.data.length;i < l; i++) {
        bitBuffer.put(this.data[i], 8);
      }
    };
    module.exports = ByteData;
  });

  // src/vendor/qrcode/kanji-data.js
  var require_kanji_data = __commonJS((exports, module) => {
    var Mode = require_mode();
    var Utils = require_utils();
    function KanjiData(data) {
      this.mode = Mode.KANJI;
      this.data = data;
    }
    KanjiData.getBitsLength = function getBitsLength(length) {
      return length * 13;
    };
    KanjiData.prototype.getLength = function getLength() {
      return this.data.length;
    };
    KanjiData.prototype.getBitsLength = function getBitsLength() {
      return KanjiData.getBitsLength(this.data.length);
    };
    KanjiData.prototype.write = function(bitBuffer) {
      let i;
      for (i = 0;i < this.data.length; i++) {
        let value = Utils.toSJIS(this.data[i]);
        if (value >= 33088 && value <= 40956) {
          value -= 33088;
        } else if (value >= 57408 && value <= 60351) {
          value -= 49472;
        } else {
          throw new Error("Invalid SJIS character: " + this.data[i] + `
` + "Make sure your charset is UTF-8");
        }
        value = (value >>> 8 & 255) * 192 + (value & 255);
        bitBuffer.put(value, 13);
      }
    };
    module.exports = KanjiData;
  });

  // src/vendor/dijkstrajs/dijkstra.js
  var require_dijkstra = __commonJS((exports, module) => {
    var dijkstra = {
      single_source_shortest_paths: function(graph, s, d) {
        var predecessors = {};
        var costs = {};
        costs[s] = 0;
        var open = dijkstra.PriorityQueue.make();
        open.push(s, 0);
        var closest, u, v, cost_of_s_to_u, adjacent_nodes, cost_of_e, cost_of_s_to_u_plus_cost_of_e, cost_of_s_to_v, first_visit;
        while (!open.empty()) {
          closest = open.pop();
          u = closest.value;
          cost_of_s_to_u = closest.cost;
          adjacent_nodes = graph[u] || {};
          for (v in adjacent_nodes) {
            if (adjacent_nodes.hasOwnProperty(v)) {
              cost_of_e = adjacent_nodes[v];
              cost_of_s_to_u_plus_cost_of_e = cost_of_s_to_u + cost_of_e;
              cost_of_s_to_v = costs[v];
              first_visit = typeof costs[v] === "undefined";
              if (first_visit || cost_of_s_to_v > cost_of_s_to_u_plus_cost_of_e) {
                costs[v] = cost_of_s_to_u_plus_cost_of_e;
                open.push(v, cost_of_s_to_u_plus_cost_of_e);
                predecessors[v] = u;
              }
            }
          }
        }
        if (typeof d !== "undefined" && typeof costs[d] === "undefined") {
          var msg = ["Could not find a path from ", s, " to ", d, "."].join("");
          throw new Error(msg);
        }
        return predecessors;
      },
      extract_shortest_path_from_predecessor_list: function(predecessors, d) {
        var nodes = [];
        var u = d;
        var predecessor;
        while (u) {
          nodes.push(u);
          predecessor = predecessors[u];
          u = predecessors[u];
        }
        nodes.reverse();
        return nodes;
      },
      find_path: function(graph, s, d) {
        var predecessors = dijkstra.single_source_shortest_paths(graph, s, d);
        return dijkstra.extract_shortest_path_from_predecessor_list(predecessors, d);
      },
      PriorityQueue: {
        make: function(opts) {
          var T = dijkstra.PriorityQueue, t = {}, key;
          opts = opts || {};
          for (key in T) {
            if (T.hasOwnProperty(key)) {
              t[key] = T[key];
            }
          }
          t.queue = [];
          t.sorter = opts.sorter || T.default_sorter;
          return t;
        },
        default_sorter: function(a, b) {
          return a.cost - b.cost;
        },
        push: function(value, cost) {
          var item = { value, cost };
          this.queue.push(item);
          this.queue.sort(this.sorter);
        },
        pop: function() {
          return this.queue.shift();
        },
        empty: function() {
          return this.queue.length === 0;
        }
      }
    };
    if (typeof module !== "undefined") {
      module.exports = dijkstra;
    }
  });

  // src/vendor/qrcode/segments.js
  var require_segments = __commonJS((exports) => {
    var Mode = require_mode();
    var NumericData = require_numeric_data();
    var AlphanumericData = require_alphanumeric_data();
    var ByteData = require_byte_data();
    var KanjiData = require_kanji_data();
    var Regex = require_regex();
    var Utils = require_utils();
    var dijkstra = require_dijkstra();
    function getStringByteLength(str) {
      return unescape(encodeURIComponent(str)).length;
    }
    function getSegments(regex, mode, str) {
      const segments = [];
      let result;
      while ((result = regex.exec(str)) !== null) {
        segments.push({
          data: result[0],
          index: result.index,
          mode,
          length: result[0].length
        });
      }
      return segments;
    }
    function getSegmentsFromString(dataStr) {
      const numSegs = getSegments(Regex.NUMERIC, Mode.NUMERIC, dataStr);
      const alphaNumSegs = getSegments(Regex.ALPHANUMERIC, Mode.ALPHANUMERIC, dataStr);
      let byteSegs;
      let kanjiSegs;
      if (Utils.isKanjiModeEnabled()) {
        byteSegs = getSegments(Regex.BYTE, Mode.BYTE, dataStr);
        kanjiSegs = getSegments(Regex.KANJI, Mode.KANJI, dataStr);
      } else {
        byteSegs = getSegments(Regex.BYTE_KANJI, Mode.BYTE, dataStr);
        kanjiSegs = [];
      }
      const segs = numSegs.concat(alphaNumSegs, byteSegs, kanjiSegs);
      return segs.sort(function(s1, s2) {
        return s1.index - s2.index;
      }).map(function(obj) {
        return {
          data: obj.data,
          mode: obj.mode,
          length: obj.length
        };
      });
    }
    function getSegmentBitsLength(length, mode) {
      switch (mode) {
        case Mode.NUMERIC:
          return NumericData.getBitsLength(length);
        case Mode.ALPHANUMERIC:
          return AlphanumericData.getBitsLength(length);
        case Mode.KANJI:
          return KanjiData.getBitsLength(length);
        case Mode.BYTE:
          return ByteData.getBitsLength(length);
      }
    }
    function mergeSegments(segs) {
      return segs.reduce(function(acc, curr) {
        const prevSeg = acc.length - 1 >= 0 ? acc[acc.length - 1] : null;
        if (prevSeg && prevSeg.mode === curr.mode) {
          acc[acc.length - 1].data += curr.data;
          return acc;
        }
        acc.push(curr);
        return acc;
      }, []);
    }
    function buildNodes(segs) {
      const nodes = [];
      for (let i = 0;i < segs.length; i++) {
        const seg = segs[i];
        switch (seg.mode) {
          case Mode.NUMERIC:
            nodes.push([
              seg,
              { data: seg.data, mode: Mode.ALPHANUMERIC, length: seg.length },
              { data: seg.data, mode: Mode.BYTE, length: seg.length }
            ]);
            break;
          case Mode.ALPHANUMERIC:
            nodes.push([
              seg,
              { data: seg.data, mode: Mode.BYTE, length: seg.length }
            ]);
            break;
          case Mode.KANJI:
            nodes.push([
              seg,
              { data: seg.data, mode: Mode.BYTE, length: getStringByteLength(seg.data) }
            ]);
            break;
          case Mode.BYTE:
            nodes.push([
              { data: seg.data, mode: Mode.BYTE, length: getStringByteLength(seg.data) }
            ]);
        }
      }
      return nodes;
    }
    function buildGraph(nodes, version) {
      const table = {};
      const graph = { start: {} };
      let prevNodeIds = ["start"];
      for (let i = 0;i < nodes.length; i++) {
        const nodeGroup = nodes[i];
        const currentNodeIds = [];
        for (let j = 0;j < nodeGroup.length; j++) {
          const node = nodeGroup[j];
          const key = "" + i + j;
          currentNodeIds.push(key);
          table[key] = { node, lastCount: 0 };
          graph[key] = {};
          for (let n = 0;n < prevNodeIds.length; n++) {
            const prevNodeId = prevNodeIds[n];
            if (table[prevNodeId] && table[prevNodeId].node.mode === node.mode) {
              graph[prevNodeId][key] = getSegmentBitsLength(table[prevNodeId].lastCount + node.length, node.mode) - getSegmentBitsLength(table[prevNodeId].lastCount, node.mode);
              table[prevNodeId].lastCount += node.length;
            } else {
              if (table[prevNodeId])
                table[prevNodeId].lastCount = node.length;
              graph[prevNodeId][key] = getSegmentBitsLength(node.length, node.mode) + 4 + Mode.getCharCountIndicator(node.mode, version);
            }
          }
        }
        prevNodeIds = currentNodeIds;
      }
      for (let n = 0;n < prevNodeIds.length; n++) {
        graph[prevNodeIds[n]].end = 0;
      }
      return { map: graph, table };
    }
    function buildSingleSegment(data, modesHint) {
      let mode;
      const bestMode = Mode.getBestModeForData(data);
      mode = Mode.from(modesHint, bestMode);
      if (mode !== Mode.BYTE && mode.bit < bestMode.bit) {
        throw new Error('"' + data + '"' + " cannot be encoded with mode " + Mode.toString(mode) + `.
 Suggested mode is: ` + Mode.toString(bestMode));
      }
      if (mode === Mode.KANJI && !Utils.isKanjiModeEnabled()) {
        mode = Mode.BYTE;
      }
      switch (mode) {
        case Mode.NUMERIC:
          return new NumericData(data);
        case Mode.ALPHANUMERIC:
          return new AlphanumericData(data);
        case Mode.KANJI:
          return new KanjiData(data);
        case Mode.BYTE:
          return new ByteData(data);
      }
    }
    exports.fromArray = function fromArray(array) {
      return array.reduce(function(acc, seg) {
        if (typeof seg === "string") {
          acc.push(buildSingleSegment(seg, null));
        } else if (seg.data) {
          acc.push(buildSingleSegment(seg.data, seg.mode));
        }
        return acc;
      }, []);
    };
    exports.fromString = function fromString(data, version) {
      const segs = getSegmentsFromString(data, Utils.isKanjiModeEnabled());
      const nodes = buildNodes(segs);
      const graph = buildGraph(nodes, version);
      const path = dijkstra.find_path(graph.map, "start", "end");
      const optimizedSegs = [];
      for (let i = 1;i < path.length - 1; i++) {
        optimizedSegs.push(graph.table[path[i]].node);
      }
      return exports.fromArray(mergeSegments(optimizedSegs));
    };
    exports.rawSplit = function rawSplit(data) {
      return exports.fromArray(getSegmentsFromString(data, Utils.isKanjiModeEnabled()));
    };
  });

  // src/vendor/qrcode/qrcode.js
  var require_qrcode = __commonJS((exports) => {
    var Utils = require_utils();
    var ECLevel = require_error_correction_level();
    var BitBuffer = require_bit_buffer();
    var BitMatrix = require_bit_matrix();
    var AlignmentPattern = require_alignment_pattern();
    var FinderPattern = require_finder_pattern();
    var MaskPattern = require_mask_pattern();
    var ECCode = require_error_correction_code();
    var ReedSolomonEncoder = require_reed_solomon_encoder();
    var Version = require_version();
    var FormatInfo = require_format_info();
    var Mode = require_mode();
    var Segments = require_segments();
    function setupFinderPattern(matrix, version) {
      const size = matrix.size;
      const pos = FinderPattern.getPositions(version);
      for (let i = 0;i < pos.length; i++) {
        const row = pos[i][0];
        const col = pos[i][1];
        for (let r = -1;r <= 7; r++) {
          if (row + r <= -1 || size <= row + r)
            continue;
          for (let c = -1;c <= 7; c++) {
            if (col + c <= -1 || size <= col + c)
              continue;
            if (r >= 0 && r <= 6 && (c === 0 || c === 6) || c >= 0 && c <= 6 && (r === 0 || r === 6) || r >= 2 && r <= 4 && c >= 2 && c <= 4) {
              matrix.set(row + r, col + c, true, true);
            } else {
              matrix.set(row + r, col + c, false, true);
            }
          }
        }
      }
    }
    function setupTimingPattern(matrix) {
      const size = matrix.size;
      for (let r = 8;r < size - 8; r++) {
        const value = r % 2 === 0;
        matrix.set(r, 6, value, true);
        matrix.set(6, r, value, true);
      }
    }
    function setupAlignmentPattern(matrix, version) {
      const pos = AlignmentPattern.getPositions(version);
      for (let i = 0;i < pos.length; i++) {
        const row = pos[i][0];
        const col = pos[i][1];
        for (let r = -2;r <= 2; r++) {
          for (let c = -2;c <= 2; c++) {
            if (r === -2 || r === 2 || c === -2 || c === 2 || r === 0 && c === 0) {
              matrix.set(row + r, col + c, true, true);
            } else {
              matrix.set(row + r, col + c, false, true);
            }
          }
        }
      }
    }
    function setupVersionInfo(matrix, version) {
      const size = matrix.size;
      const bits = Version.getEncodedBits(version);
      let row, col, mod;
      for (let i = 0;i < 18; i++) {
        row = Math.floor(i / 3);
        col = i % 3 + size - 8 - 3;
        mod = (bits >> i & 1) === 1;
        matrix.set(row, col, mod, true);
        matrix.set(col, row, mod, true);
      }
    }
    function setupFormatInfo(matrix, errorCorrectionLevel, maskPattern) {
      const size = matrix.size;
      const bits = FormatInfo.getEncodedBits(errorCorrectionLevel, maskPattern);
      let i, mod;
      for (i = 0;i < 15; i++) {
        mod = (bits >> i & 1) === 1;
        if (i < 6) {
          matrix.set(i, 8, mod, true);
        } else if (i < 8) {
          matrix.set(i + 1, 8, mod, true);
        } else {
          matrix.set(size - 15 + i, 8, mod, true);
        }
        if (i < 8) {
          matrix.set(8, size - i - 1, mod, true);
        } else if (i < 9) {
          matrix.set(8, 15 - i - 1 + 1, mod, true);
        } else {
          matrix.set(8, 15 - i - 1, mod, true);
        }
      }
      matrix.set(size - 8, 8, 1, true);
    }
    function setupData(matrix, data) {
      const size = matrix.size;
      let inc = -1;
      let row = size - 1;
      let bitIndex = 7;
      let byteIndex = 0;
      for (let col = size - 1;col > 0; col -= 2) {
        if (col === 6)
          col--;
        while (true) {
          for (let c = 0;c < 2; c++) {
            if (!matrix.isReserved(row, col - c)) {
              let dark = false;
              if (byteIndex < data.length) {
                dark = (data[byteIndex] >>> bitIndex & 1) === 1;
              }
              matrix.set(row, col - c, dark);
              bitIndex--;
              if (bitIndex === -1) {
                byteIndex++;
                bitIndex = 7;
              }
            }
          }
          row += inc;
          if (row < 0 || size <= row) {
            row -= inc;
            inc = -inc;
            break;
          }
        }
      }
    }
    function createData(version, errorCorrectionLevel, segments) {
      const buffer = new BitBuffer;
      segments.forEach(function(data) {
        buffer.put(data.mode.bit, 4);
        buffer.put(data.getLength(), Mode.getCharCountIndicator(data.mode, version));
        data.write(buffer);
      });
      const totalCodewords = Utils.getSymbolTotalCodewords(version);
      const ecTotalCodewords = ECCode.getTotalCodewordsCount(version, errorCorrectionLevel);
      const dataTotalCodewordsBits = (totalCodewords - ecTotalCodewords) * 8;
      if (buffer.getLengthInBits() + 4 <= dataTotalCodewordsBits) {
        buffer.put(0, 4);
      }
      while (buffer.getLengthInBits() % 8 !== 0) {
        buffer.putBit(0);
      }
      const remainingByte = (dataTotalCodewordsBits - buffer.getLengthInBits()) / 8;
      for (let i = 0;i < remainingByte; i++) {
        buffer.put(i % 2 ? 17 : 236, 8);
      }
      return createCodewords(buffer, version, errorCorrectionLevel);
    }
    function createCodewords(bitBuffer, version, errorCorrectionLevel) {
      const totalCodewords = Utils.getSymbolTotalCodewords(version);
      const ecTotalCodewords = ECCode.getTotalCodewordsCount(version, errorCorrectionLevel);
      const dataTotalCodewords = totalCodewords - ecTotalCodewords;
      const ecTotalBlocks = ECCode.getBlocksCount(version, errorCorrectionLevel);
      const blocksInGroup2 = totalCodewords % ecTotalBlocks;
      const blocksInGroup1 = ecTotalBlocks - blocksInGroup2;
      const totalCodewordsInGroup1 = Math.floor(totalCodewords / ecTotalBlocks);
      const dataCodewordsInGroup1 = Math.floor(dataTotalCodewords / ecTotalBlocks);
      const dataCodewordsInGroup2 = dataCodewordsInGroup1 + 1;
      const ecCount = totalCodewordsInGroup1 - dataCodewordsInGroup1;
      const rs = new ReedSolomonEncoder(ecCount);
      let offset = 0;
      const dcData = new Array(ecTotalBlocks);
      const ecData = new Array(ecTotalBlocks);
      let maxDataSize = 0;
      const buffer = new Uint8Array(bitBuffer.buffer);
      for (let b = 0;b < ecTotalBlocks; b++) {
        const dataSize = b < blocksInGroup1 ? dataCodewordsInGroup1 : dataCodewordsInGroup2;
        dcData[b] = buffer.slice(offset, offset + dataSize);
        ecData[b] = rs.encode(dcData[b]);
        offset += dataSize;
        maxDataSize = Math.max(maxDataSize, dataSize);
      }
      const data = new Uint8Array(totalCodewords);
      let index = 0;
      let i, r;
      for (i = 0;i < maxDataSize; i++) {
        for (r = 0;r < ecTotalBlocks; r++) {
          if (i < dcData[r].length) {
            data[index++] = dcData[r][i];
          }
        }
      }
      for (i = 0;i < ecCount; i++) {
        for (r = 0;r < ecTotalBlocks; r++) {
          data[index++] = ecData[r][i];
        }
      }
      return data;
    }
    function createSymbol(data, version, errorCorrectionLevel, maskPattern) {
      let segments;
      if (Array.isArray(data)) {
        segments = Segments.fromArray(data);
      } else if (typeof data === "string") {
        let estimatedVersion = version;
        if (!estimatedVersion) {
          const rawSegments = Segments.rawSplit(data);
          estimatedVersion = Version.getBestVersionForData(rawSegments, errorCorrectionLevel);
        }
        segments = Segments.fromString(data, estimatedVersion || 40);
      } else {
        throw new Error("Invalid data");
      }
      const bestVersion = Version.getBestVersionForData(segments, errorCorrectionLevel);
      if (!bestVersion) {
        throw new Error("The amount of data is too big to be stored in a QR Code");
      }
      if (!version) {
        version = bestVersion;
      } else if (version < bestVersion) {
        throw new Error(`
` + `The chosen QR Code version cannot contain this amount of data.
` + "Minimum version required to store current data is: " + bestVersion + `.
`);
      }
      const dataBits = createData(version, errorCorrectionLevel, segments);
      const moduleCount = Utils.getSymbolSize(version);
      const modules = new BitMatrix(moduleCount);
      setupFinderPattern(modules, version);
      setupTimingPattern(modules);
      setupAlignmentPattern(modules, version);
      setupFormatInfo(modules, errorCorrectionLevel, 0);
      if (version >= 7) {
        setupVersionInfo(modules, version);
      }
      setupData(modules, dataBits);
      if (isNaN(maskPattern)) {
        maskPattern = MaskPattern.getBestMask(modules, setupFormatInfo.bind(null, modules, errorCorrectionLevel));
      }
      MaskPattern.applyMask(maskPattern, modules);
      setupFormatInfo(modules, errorCorrectionLevel, maskPattern);
      return {
        modules,
        version,
        errorCorrectionLevel,
        maskPattern,
        segments
      };
    }
    exports.create = function create(data, options) {
      if (typeof data === "undefined" || data === "") {
        throw new Error("No input text");
      }
      let errorCorrectionLevel = ECLevel.M;
      let version;
      let mask;
      if (typeof options !== "undefined") {
        errorCorrectionLevel = ECLevel.from(options.errorCorrectionLevel, ECLevel.M);
        version = Version.from(options.version);
        mask = MaskPattern.from(options.maskPattern);
        if (options.toSJISFunc) {
          Utils.setToSJISFunction(options.toSJISFunc);
        }
      }
      return createSymbol(data, version, errorCorrectionLevel, mask);
    };
  });

  // src/pico-fonts.ts
  var PICO_FONT_IDS = [
    "tiny5",
    "tom-thumb",
    "x11-4x6",
    "kenney-mini",
    "m3x6",
    "x11-5x7",
    "spleen-5x8",
    "m5x7",
    "monogram",
    "kenney-pixel"
  ];
  var PICO_FONTS = [
    {
      id: "tiny5",
      name: "Tiny5",
      family: "Pico Tiny5",
      rows: 7,
      capHeight: 5,
      xHeight: 4,
      by: "The Tiny5 Project Authors (Gissio)",
      license: "SIL Open Font License 1.1",
      licenseFile: "tiny5-OFL.txt"
    },
    {
      id: "tom-thumb",
      name: "Tom Thumb",
      family: "Pico Tom Thumb",
      rows: 6,
      capHeight: 5,
      xHeight: 4,
      by: "Brian Swetland and Robey Pointer",
      license: "CC0 1.0 (also offered under MIT and CC BY 3.0)"
    },
    {
      id: "x11-4x6",
      name: "X11 fixed 4x6",
      family: "Pico X11 fixed 4x6",
      rows: 6,
      capHeight: 5,
      xHeight: 4,
      by: "the X Consortium fixed fonts",
      license: "Public domain"
    },
    {
      id: "kenney-mini",
      name: "Kenney Mini",
      family: "Pico Kenney Mini",
      rows: 7,
      capHeight: 5,
      xHeight: 4,
      by: "Kenney (www.kenney.nl)",
      license: "CC0 1.0"
    },
    {
      id: "m3x6",
      name: "m3x6",
      family: "Pico m3x6",
      rows: 8,
      capHeight: 6,
      xHeight: 5,
      by: "Daniel Linssen",
      license: "Free to use with attribution"
    },
    {
      id: "x11-5x7",
      name: "X11 fixed 5x7",
      family: "Pico X11 fixed 5x7",
      rows: 7,
      capHeight: 6,
      xHeight: 4,
      by: "the X Consortium fixed fonts",
      license: "Public domain"
    },
    {
      id: "spleen-5x8",
      name: "Spleen 5x8",
      family: "Pico Spleen 5x8",
      rows: 8,
      capHeight: 6,
      xHeight: 5,
      by: "Frederic Cambus",
      license: "BSD 2-Clause",
      licenseFile: "spleen-LICENSE.txt"
    },
    {
      id: "m5x7",
      name: "m5x7",
      family: "Pico m5x7",
      rows: 9,
      capHeight: 7,
      xHeight: 5,
      by: "Daniel Linssen",
      license: "CC0 1.0 (attribution appreciated)"
    },
    {
      id: "monogram",
      name: "monogram",
      family: "Pico monogram",
      rows: 9,
      capHeight: 7,
      xHeight: 5,
      by: "datagoblin",
      license: "CC0 1.0"
    },
    {
      id: "kenney-pixel",
      name: "Kenney Pixel",
      family: "Pico Kenney Pixel",
      rows: 9,
      capHeight: 7,
      xHeight: 6,
      by: "Kenney (www.kenney.nl)",
      license: "CC0 1.0"
    }
  ];
  var PICO_DEFAULT_FONT = "tiny5";

  // src/pico-font-preference.ts
  var PICO_FONT_KEY = "pico.font";
  function isPicoFont(value) {
    return PICO_FONT_IDS.includes(value);
  }
  function picoFontFrom(stored) {
    return isPicoFont(stored) ? stored : PICO_DEFAULT_FONT;
  }

  // src/model.ts
  var cache = new WeakMap;
  function picoModelFrom(model) {
    const cached = cache.get(model);
    if (cached !== undefined)
      return cached;
    const catalog = model.catalog._tag === "Ready" ? { _tag: "Ready", games: model.catalog.window } : model.catalog;
    const shaped = { ...model, catalog };
    cache.set(model, shaped);
    return shaped;
  }

  // src/pico-home-view.ts
  function picoHomeViewFromCatalog(catalog) {
    const cached = homeViews.get(catalog);
    if (cached !== undefined)
      return cached;
    const view = homeViewFrom(catalog);
    homeViews.set(catalog, view);
    return view;
  }
  var homeViews = new WeakMap;
  function homeViewFrom(catalog) {
    switch (catalog._tag) {
      case "Loading":
        return { _tag: "Loading" };
      case "Error":
        return { _tag: "Failed", message: catalog.message };
      case "Empty":
        return { _tag: "Empty" };
      case "Ready":
        return catalog.games.length === 0 ? { _tag: "Empty" } : {
          _tag: "Shelf",
          games: catalog.games.map((game) => ({
            id: game.id,
            title: game.title,
            ...game.subtitle === undefined ? {} : { subtitle: game.subtitle },
            ...game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl },
            ...game.resumable === undefined ? {} : { resumable: game.resumable },
            ...game.section === undefined ? {} : { section: game.section },
            ...game.lastPlayedAt === undefined ? {} : { lastPlayedAt: game.lastPlayedAt },
            ...game.playCount === undefined ? {} : { playCount: game.playCount },
            ...game.totalPlaytimeSeconds === undefined ? {} : { totalPlaytimeSeconds: game.totalPlaytimeSeconds },
            ...game.download === undefined ? {} : {
              download: {
                ...game.download.bytes === undefined ? {} : { bytes: game.download.bytes },
                ...game.download.queued === true ? { queued: true } : {},
                ...game.download.progress === undefined ? {} : {
                  progress: {
                    done: game.download.progress.done,
                    ...game.download.progress.total === undefined ? {} : { total: game.download.progress.total }
                  }
                }
              }
            },
            ...game.launchLocations === undefined ? {} : {
              locations: game.launchLocations.map((location) => ({
                id: location.id,
                label: location.label
              }))
            }
          }))
        };
    }
  }

  // src/pico-library-view.ts
  var PICO_ALL_SECTIONS = "ALL";
  var PICO_ORDERS = ["korri", "title", "played", "recent"];
  var PICO_ORDER_LABELS = {
    korri: "KORRI",
    title: "A-Z",
    played: "MOST PLAYED",
    recent: "RECENT"
  };
  function picoCollectionsFrom(games) {
    const collections = [];
    for (const game of games) {
      const title = (game.section ?? "GAMES").toUpperCase();
      const existing = collections.find((candidate) => candidate.title === title);
      if (existing === undefined)
        collections.push({ title, games: [game] });
      else
        existing.games.push(game);
    }
    return collections;
  }
  function picoHeroPick(games) {
    const timed = games.filter((game) => game.lastPlayedAt !== undefined);
    const latest = timed.reduce((best, game) => best === undefined || (game.lastPlayedAt ?? 0) > (best.lastPlayedAt ?? 0) ? game : best, undefined);
    if (latest !== undefined)
      return { game: latest, reason: "LAST PLAYED" };
    const first = games[0];
    return first === undefined ? undefined : { game: first };
  }

  // src/pico-text-draft.ts
  var picoTextDraft = (text) => ({ text, capitals: false, symbols: false });
  function picoDraftType(draft, character, maxLength) {
    return maxLength !== undefined && [...draft.text].length >= maxLength ? draft : { ...draft, text: draft.text + character };
  }
  var picoDraftBackspace = (draft) => ({ ...draft, text: [...draft.text].slice(0, -1).join("") });

  // src/state/wifi.ts
  var WIFI_SSID_MAX = 32;
  var WIFI_PASSWORD_MAX = 63;
  var NOTHING = { _tag: "Nothing" };
  var passwordEntry = (network) => ({ _tag: "Password", networkId: network.id, ssid: network.ssid, draft: picoTextDraft(""), shown: false });
  var HIDDEN_NAME_ENTRY = { _tag: "HiddenName", draft: picoTextDraft("") };
  function scannedNetworks(korri) {
    const wifi = korri.network?.wifi;
    return wifi?._tag === "Available" ? wifi.networks : [];
  }
  function lastAttempt(korri) {
    const wifi = korri.network?.wifi;
    return wifi?._tag === "Available" ? wifi.attempt : { _tag: "Idle" };
  }
  function onNetwork(korri, ssid) {
    const connection = korri.network?.connection;
    return connection?._tag === "Wifi" && connection.ssid === ssid;
  }
  function pickNetwork(korri, networkId) {
    const network = scannedNetworks(korri).find((candidate) => candidate.id === networkId);
    if (network === undefined || onNetwork(korri, network.ssid) || lastAttempt(korri)._tag === "Connecting")
      return NOTHING;
    return network.secured && !network.known ? { _tag: "Edit", entry: passwordEntry(network) } : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } };
  }
  function retryAttempt(korri) {
    const attempt = lastAttempt(korri);
    if (attempt._tag !== "Failed")
      return NOTHING;
    const network = scannedNetworks(korri).find((candidate) => candidate.ssid === attempt.ssid);
    if (network === undefined) {
      return { _tag: "Edit", entry: { _tag: "HiddenPassword", ssid: attempt.ssid, draft: picoTextDraft(""), shown: false } };
    }
    return network.secured && (attempt.passwordRejected || !network.known) ? { _tag: "Edit", entry: passwordEntry(network) } : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: network.id } };
  }
  function submitEntry(entry) {
    switch (entry._tag) {
      case "HiddenName": {
        const ssid = entry.draft.text.trim();
        return ssid === "" ? NOTHING : { _tag: "Edit", entry: { _tag: "HiddenPassword", ssid, draft: picoTextDraft(""), shown: false } };
      }
      case "HiddenPassword":
        return {
          _tag: "Send",
          request: entry.draft.text === "" ? { _tag: "ConnectHiddenWifi", ssid: entry.ssid } : { _tag: "ConnectHiddenWifi", ssid: entry.ssid, password: entry.draft.text }
        };
      case "Password":
        return entry.draft.text === "" ? NOTHING : { _tag: "Send", request: { _tag: "ConnectWifi", networkId: entry.networkId, password: entry.draft.text } };
    }
  }
  function entryReady(entry) {
    return submitEntry(entry)._tag !== "Nothing";
  }
  var entryMaxLength = (entry) => entry._tag === "HiddenName" ? WIFI_SSID_MAX : WIFI_PASSWORD_MAX;
  var editEntry = (entry, change) => ({ ...entry, draft: change(entry.draft) });
  var toggleShown = (entry) => entry._tag === "HiddenName" ? entry : { ...entry, shown: !entry.shown };
  function sameAttempt(a, b) {
    if (b === undefined || a._tag !== b._tag)
      return false;
    switch (a._tag) {
      case "Idle":
        return true;
      case "Connecting":
        return b._tag === "Connecting" && a.ssid === b.ssid;
      case "Failed":
        return b._tag === "Failed" && a.ssid === b.ssid && a.message === b.message && a.passwordRejected === b.passwordRejected;
    }
  }

  // src/pico-wifi-view.ts
  var PICO_SIGNAL_BAR_ABOVE = [5, 30, 55, 80];
  function picoSignalBars(strengthPercent) {
    return PICO_SIGNAL_BAR_ABOVE.filter((floor) => strengthPercent > floor).length;
  }
  function picoNetworksFrom(korri) {
    const connection = korri.network?.connection;
    return scannedNetworks(korri).map((network) => ({
      id: network.id,
      ssid: network.ssid,
      bars: picoSignalBars(network.strengthPercent),
      secured: network.secured,
      connected: connection?._tag === "Wifi" && connection.ssid === network.ssid
    }));
  }
  function picoConnectionLabel(connection) {
    switch (connection._tag) {
      case "Offline":
        return "Offline";
      case "Wired":
        return "Cable";
      case "Wifi":
        return connection.ssid;
    }
  }
  function picoWifiEntryView(entry) {
    const base = { draft: entry.draft, maxLength: entryMaxLength(entry), ready: entryReady(entry) };
    switch (entry._tag) {
      case "HiddenName":
        return {
          ...base,
          place: "HIDDEN NETWORK",
          prompt: "Network name",
          fieldLabel: "Hidden network name",
          note: "Type the name exactly, capitals and all.",
          placeholder: "Name",
          secret: false,
          shown: true,
          submitLabel: "NEXT"
        };
      case "HiddenPassword":
        return {
          ...base,
          place: "PASSWORD",
          prompt: `Password for ${entry.ssid}`,
          fieldLabel: `Password for ${entry.ssid}`,
          note: "Leave it empty for an open network.",
          placeholder: "None",
          secret: true,
          shown: entry.shown,
          submitLabel: "CONNECT"
        };
      case "Password":
        return {
          ...base,
          place: "PASSWORD",
          prompt: `Password for ${entry.ssid}`,
          fieldLabel: `Password for ${entry.ssid}`,
          placeholder: "Password",
          secret: true,
          shown: entry.shown,
          submitLabel: "CONNECT"
        };
    }
  }

  // src/pico-settings-view.ts
  function picoTextSettingFrom(settings, settingId) {
    for (const group of settings.groups) {
      for (const row of group.rows) {
        if (row.id === settingId && row.control.kind === "text") {
          return { group: group.title, row };
        }
      }
    }
    return;
  }
  var PICO_IDENTITY_BACKUP_ACTION = "identity:backup";
  var PICO_IDENTITY_SWITCH_LOCAL_ACTION = "identity:switch:local";
  var PICO_IDENTITY_SWITCH_NIP46_ACTION = "identity:switch:nip46";
  var picoIdentityRetiredExportAction = (key) => `identity:retired:export:${key}`;
  var picoIdentityRetiredDeleteAction = (key) => `identity:retired:delete:${key}`;
  function picoIdentityRetiredKey(action, operation) {
    const prefix = `identity:retired:${operation}:`;
    return action?.startsWith(prefix) === true ? action.slice(prefix.length) : undefined;
  }
  var PICO_FONT_SETTING = "pico:font";
  var PICO_SEGMENTS_MAX = 4;
  var PICO_SEGMENTS_CHARS = 24;
  var PICO_WIFI_SCAN_ACTION = "pico:wifi:scan";
  var PICO_WIFI_HIDDEN_ACTION = "pico:wifi:hidden";
  var WIFI_NETWORK_PREFIX = "pico:wifi:network:";
  var picoWifiNetworkAction = (networkId) => `${WIFI_NETWORK_PREFIX}${networkId}`;
  function picoWifiNetworkId(actionId) {
    return actionId.startsWith(WIFI_NETWORK_PREFIX) ? actionId.slice(WIFI_NETWORK_PREFIX.length) : undefined;
  }
  function wifiGroup(model) {
    const network = model.network;
    if (network === undefined)
      return;
    const wifi = network.wifi;
    const attempt = wifi._tag === "Available" ? wifi.attempt : undefined;
    const connection = {
      id: "pico:wifi:connection",
      label: "Connection",
      value: attempt?._tag === "Connecting" ? `Joining ${attempt.ssid}…` : picoConnectionLabel(network.connection),
      control: { kind: "fact" },
      state: attempt?._tag === "Failed" ? { failed: attempt.message } : "idle"
    };
    if (wifi._tag === "NoAdapter") {
      return {
        title: "WI-FI",
        rows: [connection, { id: "pico:wifi:adapter", label: "Wi-Fi", value: "None on this device", control: { kind: "fact" }, state: "idle" }]
      };
    }
    const networks = picoNetworksFrom(model);
    return {
      title: "WI-FI",
      rows: [
        connection,
        {
          id: PICO_WIFI_SCAN_ACTION,
          label: "Scan for networks",
          value: wifi.scanning ? "Scanning…" : `${networks.length} found`,
          control: { kind: "action", actionId: PICO_WIFI_SCAN_ACTION, destructive: false },
          state: "idle"
        },
        ...networks.map((item) => ({
          id: picoWifiNetworkAction(item.id),
          label: item.ssid,
          control: { kind: "network", actionId: picoWifiNetworkAction(item.id), network: item },
          state: "idle"
        })),
        {
          id: PICO_WIFI_HIDDEN_ACTION,
          label: "Hidden network",
          description: "Join a network that does not show its name.",
          control: { kind: "action", actionId: PICO_WIFI_HIDDEN_ACTION, destructive: false },
          state: "idle"
        }
      ]
    };
  }
  function picoGroup(font) {
    const at = Math.max(0, PICO_FONTS.findIndex((candidate) => candidate.id === font));
    const current = PICO_FONTS[at];
    const next = PICO_FONTS[(at + 1) % PICO_FONTS.length];
    return {
      title: "PICO",
      rows: [{
        id: PICO_FONT_SETTING,
        label: "Font",
        description: "Kept on this device. Changes at once.",
        control: { kind: "step", next: next?.id ?? font, current: (current?.name ?? font).toUpperCase() },
        state: "idle"
      }]
    };
  }
  function picoSettingsViewFromModel(model, pico) {
    const identity = model.identityManagement;
    const identityRows = identity ? [
      ...identity.localBackupAvailable ? [{
        id: PICO_IDENTITY_BACKUP_ACTION,
        label: "Back up current identity",
        description: "Encrypted NIP-49 text and QR code",
        control: { kind: "action", actionId: PICO_IDENTITY_BACKUP_ACTION, destructive: false },
        state: "idle"
      }] : [],
      {
        id: PICO_IDENTITY_SWITCH_LOCAL_ACTION,
        label: "Switch from backup",
        control: { kind: "action", actionId: PICO_IDENTITY_SWITCH_LOCAL_ACTION, destructive: true },
        state: "idle"
      },
      {
        id: PICO_IDENTITY_SWITCH_NIP46_ACTION,
        label: "Switch to NIP-46",
        control: { kind: "action", actionId: PICO_IDENTITY_SWITCH_NIP46_ACTION, destructive: true },
        state: "idle"
      },
      ...identity.retiredPublicKeys.flatMap((publicKey) => {
        const short = `${publicKey.slice(0, 8)}…${publicKey.slice(-8)}`;
        return [{
          id: picoIdentityRetiredExportAction(publicKey),
          label: `Back up retired ${short}`,
          control: { kind: "action", actionId: picoIdentityRetiredExportAction(publicKey), destructive: false },
          state: "idle"
        }, {
          id: picoIdentityRetiredDeleteAction(publicKey),
          label: `Delete retired ${short}`,
          control: { kind: "action", actionId: picoIdentityRetiredDeleteAction(publicKey), destructive: true },
          state: "idle"
        }];
      })
    ] : [];
    const wifi = wifiGroup(model);
    return {
      groups: [
        ...model.settings.map((group) => ({
          title: group.title.toUpperCase(),
          rows: group.items.map((item) => picoSettingRowFrom(item, model))
        })),
        ...wifi === undefined ? [] : [wifi],
        ...identityRows.length ? [{ title: "IDENTITY", rows: identityRows }] : [],
        ...pico === undefined ? [] : [picoGroup(pico.font)]
      ],
      ...model.buildLabel === undefined ? {} : { buildLabel: model.buildLabel }
    };
  }
  function picoNumberRange(range) {
    if (range.min !== undefined && range.max !== undefined)
      return `${range.min} to ${range.max}`;
    if (range.min !== undefined)
      return `At least ${range.min}`;
    if (range.max !== undefined)
      return `At most ${range.max}`;
    return;
  }
  function picoNumberFits(text, range) {
    const trimmed = text.trim();
    if (trimmed === "" || !/^-?\d*\.?\d+$/.test(trimmed))
      return false;
    const value = Number(trimmed);
    if (range.step === 1 && !Number.isInteger(value))
      return false;
    return (range.min === undefined || value >= range.min) && (range.max === undefined || value <= range.max);
  }
  function picoSettingRowFrom(item, model) {
    const status = model.settingsStatus;
    const state = status._tag === "Saving" && status.settingId === item.id ? "saving" : status._tag === "Problem" && status.settingId === item.id ? { problem: status.message } : "idle";
    return {
      id: item.id,
      label: item.label,
      ...item.value === undefined ? {} : { value: item.value },
      ...item.description === undefined ? {} : { description: item.description },
      ...item.source === undefined ? {} : { source: item.source },
      ...item.scope === "device" ? { deviceOnly: true } : {},
      control: controlFor(item),
      state
    };
  }
  function controlFor(item) {
    const interaction = item.interaction;
    if (interaction === undefined)
      return { kind: "fact" };
    switch (interaction.kind) {
      case "action":
        return {
          kind: "action",
          actionId: interaction.actionId,
          destructive: interaction.destructive === true,
          ...interaction.confirmation === undefined ? {} : {
            confirmation: { ...interaction.confirmation, destructive: interaction.destructive === true }
          }
        };
      case "choice": {
        const choices = interaction.choices;
        const found = choices.findIndex((choice) => choice.value === interaction.selected);
        if (found < 0)
          return { kind: "pick", options: choices.map(({ value, label }) => ({ value, label })) };
        const current = found;
        const nextChoice = choices[(current + 1) % choices.length];
        const width = choices.reduce((sum, choice) => sum + choice.label.length + 1, 0);
        if (choices.length > PICO_SEGMENTS_MAX || width > PICO_SEGMENTS_CHARS) {
          return { kind: "step", next: nextChoice?.value ?? "", current: choices[current]?.label ?? "" };
        }
        return {
          kind: "cycle",
          next: nextChoice?.value ?? "",
          options: choices.map(({ value, label }) => ({ value, label })),
          current
        };
      }
      case "number":
        return {
          kind: "text",
          sensitive: false,
          initial: interaction.value === undefined ? "" : String(interaction.value),
          numeric: {
            ...interaction.min === undefined ? {} : { min: interaction.min },
            ...interaction.max === undefined ? {} : { max: interaction.max },
            ...interaction.step === undefined ? {} : { step: interaction.step }
          },
          placeholder: picoNumberRange(interaction) ?? "Number",
          maxLength: 24,
          ...interaction.clearable === true && interaction.value !== undefined ? { clearLabel: "Inherit" } : {}
        };
      case "text":
        return {
          kind: "text",
          sensitive: false,
          ...interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder },
          ...interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength },
          ...interaction.allowEmpty === true ? { allowEmpty: true } : {}
        };
      case "sensitiveText":
        return {
          kind: "text",
          sensitive: true,
          ...interaction.placeholder === undefined ? {} : { placeholder: interaction.placeholder },
          ...interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength },
          ...interaction.clearLabel === undefined ? {} : { clearLabel: interaction.clearLabel }
        };
    }
  }

  // src/pico-launch-view.ts
  var picoLaunchOpen = (model) => model.launchSettings !== undefined && model.launchSettings._tag !== "Closed";
  function picoLaunchViewFromModel(model) {
    const launch = model.launchSettings;
    if (launch === undefined || launch._tag === "Closed")
      return;
    const close = launch.actions.find((action) => action.id.endsWith(":cancel"));
    const layers = [];
    for (const place of launch.places) {
      const rows = place.groups.flatMap((group) => group.items.map((item, index) => {
        const row = picoSettingRowFrom(item, model);
        return index === 0 ? { ...row, section: group.title.toUpperCase() } : row;
      }));
      const last = layers.at(-1);
      const scope = { placeId: place.id, label: place.scope.label, kind: place.scope.kind, rows };
      if (last !== undefined && last.id === place.layer.id)
        last.scopes.push(scope);
      else
        layers.push({ id: place.layer.id, title: place.layer.title, scopes: [scope] });
    }
    return {
      ...launch.gameTitle === undefined ? {} : { gameTitle: launch.gameTitle },
      phase: launch._tag,
      ...launch._tag === "Conflict" || launch._tag === "Error" ? { message: launch.message } : {},
      notes: launch.notes,
      actions: launch.actions,
      ...close === undefined ? {} : { closeActionId: close.id },
      layers,
      settings: {
        groups: layers.map((layer) => ({ title: layer.title.toUpperCase(), rows: layer.scopes.flatMap((scope) => scope.rows) }))
      }
    };
  }
  var picoLaunchScope = (layer, scope) => layer === undefined ? 0 : Math.max(0, Math.min(scope, layer.scopes.length - 1));
  function picoLaunchShown(view, scope) {
    return {
      groups: view.layers.map((layer) => ({
        title: layer.title.toUpperCase(),
        rows: layer.scopes[picoLaunchScope(layer, scope)]?.rows ?? []
      }))
    };
  }

  // src/pico-overlay-view.ts
  function placesOf(range) {
    const places = (n) => {
      const text = String(n);
      const dot = text.indexOf(".");
      return dot === -1 ? 0 : text.length - dot - 1;
    };
    return Math.max(places(range.step), places(range.min));
  }
  function picoRangeLabel(value, range) {
    return value.toFixed(placesOf(range));
  }
  function picoRangeStep(range, way) {
    const next = Number((range.value + way * range.step).toFixed(placesOf(range)));
    return Math.min(range.max, Math.max(range.min, next));
  }
  function picoOverlayViewFrom(presentation, status) {
    return {
      title: presentation.title ?? "PLAYING",
      controls: presentation.controls.map(controlView),
      groups: presentation.groups.map((group) => ({
        id: group.id,
        label: group.label.toUpperCase(),
        controls: group.controls.map(controlView)
      })),
      ...status._tag === "Problem" ? { problem: { kicker: status.kicker, reason: status.reason, canRetry: status.canRetry } } : {},
      ...presentation.streamPlayers === undefined || presentation.streamPlayers.length === 0 ? {} : { streamPlayers: presentation.streamPlayers }
    };
  }
  function controlView(control) {
    const i = control.interaction;
    const base = {
      id: control.id,
      label: control.label,
      ...control.description === undefined ? {} : { description: control.description },
      enabled: control.enabled,
      ...control.disabledReason === undefined ? {} : { disabledReason: control.disabledReason },
      destructive: control.destructive
    };
    switch (i.kind) {
      case "command":
        return base;
      case "toggle":
        return {
          ...base,
          stateLabel: (i.value ? i.trueLabel : i.falseLabel).toUpperCase(),
          sends: { kind: "toggle", value: !i.value }
        };
      case "choice": {
        const at = Math.max(0, i.options.findIndex((option) => option.value === i.value));
        const next = i.options[(at + 1) % i.options.length];
        return {
          ...base,
          stateLabel: (i.options[at]?.label ?? i.value).toUpperCase(),
          sends: { kind: "choice", value: next?.value ?? i.value }
        };
      }
      case "range":
        return {
          ...base,
          stateLabel: picoRangeLabel(i.value, i),
          range: { value: i.value, min: i.min, max: i.max, step: i.step }
        };
    }
  }

  // src/pico-display-plan.ts
  var area = (display) => display.width * display.height;
  function picoDisplayPlan(displays) {
    const usable = (displays ?? []).filter((display) => display.width > 0 && display.height > 0);
    if (usable.length < 2)
      return;
    const show = usable.find((display) => display.main) ?? usable[0];
    if (show === undefined)
      return;
    const choose = usable.filter((display) => display !== show).sort((a, b) => Number(b.touch) - Number(a.touch) || area(b) - area(a))[0];
    return choose === undefined ? undefined : { show, choose };
  }

  // src/state/navigation.ts
  var STORE_PLACE = { view: "Providers", providerId: undefined, typing: undefined };
  var IDENTITY_FORM = { password: "", secret: "", bunkerUri: "", disposition: "transfer", confirmed: false };
  var INVITE_NOTICES = { noticed: [], showing: [], nextToken: 0 };
  function topLayer(nav) {
    if (nav.settings !== undefined)
      return "Settings";
    if (nav.detail !== undefined)
      return "Detail";
    if (nav.store)
      return "Store";
    if (nav.find)
      return "Find";
    return "Home";
  }
  var SETTINGS = { question: { _tag: "None" }, group: 0, scope: 0 };
  var HOME = { mode: "shelf", selectedGameId: undefined, menu: "Closed", focusOnReturn: "None", aim: undefined };
  function initialNavigation(initial) {
    return {
      home: initial?._tag === "Home" ? { ...HOME, mode: initial.mode ?? "shelf" } : HOME,
      find: initial?._tag === "Find",
      store: false,
      storePlace: STORE_PLACE,
      detail: initial?._tag === "Detail" ? { gameId: initial.gameId, question: { _tag: "None" } } : undefined,
      settings: initial?._tag === "Settings" ? SETTINGS : undefined,
      launch: undefined,
      search: {
        query: "",
        section: initial?._tag === "Find" ? initial.section ?? PICO_ALL_SECTIONS : PICO_ALL_SECTIONS,
        order: initial?._tag === "Find" ? initial.order ?? "korri" : "korri"
      },
      idle: { _tag: "Awake", activity: 0 },
      session: { _tag: "Idle" },
      invites: INVITE_NOTICES
    };
  }

  // src/state/onboarding.ts
  var ONBOARDING_STEPS = [
    { id: "hello", label: "HELLO" },
    { id: "wifi", label: "WI-FI" },
    { id: "name", label: "NAME" },
    { id: "you", label: "YOU" },
    { id: "pads", label: "PADS" },
    { id: "games", label: "GAMES" },
    { id: "play", label: "PLAY" }
  ];
  var WIFI_STEP = { showing: "Auto", question: { _tag: "None" }, pending: undefined, seenFailure: undefined };
  var initialOnboarding = () => ({
    step: "hello",
    wifi: WIFI_STEP,
    continuedOffline: false,
    name: { draft: undefined, saving: "No" },
    person: { draft: undefined, saving: "No" },
    identity: { _tag: "Choose" },
    backup: undefined,
    finishing: false
  });

  // src/pico-onboarding-view.ts
  var DEVICE_NAME_SETTING = "device-name";
  var PERSON_NAME_SETTING = "person-name";
  var PLAYER_SEATS_SETTING = "host.preferences.playerCount";
  var GAME_RESCAN_ACTION = "game-folder-rescan";
  var UNNAMED_DEVICE = "This device";
  function settingItem(korri, settingId) {
    for (const group of korri.settings) {
      const item = group.items.find((candidate) => candidate.id === settingId);
      if (item !== undefined)
        return item;
    }
    return;
  }
  var deviceName = (korri) => settingItem(korri, DEVICE_NAME_SETTING)?.value;
  var deviceNameSetting = (korri) => textSetting(korri, DEVICE_NAME_SETTING);
  var personName = (korri) => settingItem(korri, PERSON_NAME_SETTING)?.value;
  var personNameSetting = (korri) => textSetting(korri, PERSON_NAME_SETTING);
  function textSetting(korri, settingId) {
    const interaction = settingItem(korri, settingId)?.interaction;
    if (interaction?.kind !== "text")
      return;
    return interaction.maxLength === undefined ? {} : { maxLength: interaction.maxLength };
  }
  function canRescanGames(korri) {
    const interaction = settingItem(korri, GAME_RESCAN_ACTION)?.interaction;
    return interaction?.kind === "action" && interaction.actionId === GAME_RESCAN_ACTION;
  }
  function settingProblem(korri, settingId) {
    const status = korri.settingsStatus;
    return status._tag === "Problem" && status.settingId === settingId ? status.message : undefined;
  }
  var settingSaving = (korri, settingId) => korri.settingsStatus._tag === "Saving" && korri.settingsStatus.settingId === settingId;
  var isOffline = (korri) => korri.network?.connection._tag === "Offline";
  function asksForWifi(korri) {
    const network = korri.network;
    return network !== undefined && network.wifi._tag === "Available" && network.connection._tag !== "Wired";
  }
  function picoWifiView(step, korri) {
    const network = korri.network;
    if (step.question._tag === "Warning")
      return { _tag: "OfflineWarning" };
    if (network === undefined)
      return { _tag: "NoAdapter" };
    const { connection, wifi } = network;
    const attempt = lastAttempt(korri);
    if (step.pending?._tag === "Join")
      return { _tag: "Connecting", ssid: step.pending.ssid };
    if (attempt._tag === "Connecting")
      return { _tag: "Connecting", ssid: attempt.ssid };
    if (wifi._tag === "NoAdapter" && connection._tag !== "Wired")
      return { _tag: "NoAdapter" };
    if (attempt._tag === "Failed" && !sameAttempt(attempt, step.seenFailure)) {
      return {
        _tag: "Failed",
        ssid: attempt.ssid,
        message: attempt.message,
        passwordRejected: attempt.passwordRejected,
        fallback: connection._tag === "Offline" ? { _tag: "Offline" } : { _tag: "Stay", label: connection._tag === "Wired" ? "Stay on cable" : `Stay on ${connection.ssid}` }
      };
    }
    if (step.showing === "Auto" && connection._tag === "Wired")
      return { _tag: "Cable", wifi: wifi._tag === "Available" };
    if (step.showing === "Auto" && connection._tag === "Wifi")
      return { _tag: "Connected", ssid: connection.ssid };
    if (wifi._tag === "NoAdapter")
      return { _tag: "Cable", wifi: false };
    if (step.pending?._tag === "Scan" || wifi.scanning)
      return { _tag: "Scanning" };
    const networks = picoNetworksFrom(korri);
    if (networks.length === 0)
      return { _tag: "NoNetworks" };
    return connection._tag === "Wifi" ? { _tag: "Networks", networks, connectedSsid: connection.ssid } : { _tag: "Networks", networks };
  }
  function wifiDone(view) {
    return view._tag === "Connected" || view._tag === "Cable" || view._tag === "NoAdapter";
  }
  var cut = (text, keep) => [...text].length > keep ? `${[...text].slice(0, keep).join("")}…` : text;
  function choice(korri, note) {
    if (korri.identityManagement === undefined)
      return { enabled: false, note: "This device cannot change its identity." };
    if (isOffline(korri))
      return { enabled: false, note: "Needs a connection." };
    return { enabled: true, note };
  }
  function identityView(identity, korri) {
    const status = korri.identityManagement?.status;
    switch (identity._tag) {
      case "Choose":
        return {
          _tag: "Choose",
          offline: isOffline(korri),
          restore: choice(korri, "You have an ncryptsec backup and its password."),
          signer: choice(korri, "NIP-46: the key stays in your signer app.")
        };
      case "Ready":
        return {
          _tag: "Ready",
          origin: identity.origin,
          canBackUp: identity.origin === "created" && korri.identityManagement?.localBackupAvailable === true
        };
      case "Restore": {
        if (identity.sent && status?._tag === "Working")
          return { _tag: "Working", label: status.operation };
        const problem = identity.sent && status?._tag === "Problem" ? status.message : undefined;
        return {
          _tag: "Restore",
          backup: cut(identity.backup, 18),
          passwordLength: [...identity.password].length,
          confirmed: identity.confirmed,
          ready: identity.confirmed && identity.backup.trim() !== "" && identity.password !== "",
          ...problem === undefined ? {} : { problem }
        };
      }
      case "Signer": {
        if (identity.sent && status?._tag === "Working")
          return { _tag: "Approving", uri: cut(identity.uri, 22) };
        const problem = identity.sent && status?._tag === "Problem" ? status.message : undefined;
        return {
          _tag: "Signer",
          uri: cut(identity.uri, 22),
          confirmed: identity.confirmed,
          ready: identity.confirmed && identity.uri.trim() !== "",
          ...problem === undefined ? {} : { problem }
        };
      }
    }
  }
  function identityEntry(identity) {
    if (identity._tag === "Restore" && identity.editing !== undefined) {
      const { draft, field, shown } = identity.editing;
      return field === "password" ? {
        place: "BACKUP PASSWORD",
        prompt: "Backup password",
        fieldLabel: "Backup password",
        placeholder: "Password",
        secret: true,
        shown,
        draft,
        submitLabel: "OK",
        ready: true,
        busy: false
      } : {
        place: "BACKUP",
        prompt: "Encrypted backup",
        fieldLabel: "Encrypted backup",
        note: "It starts with ncryptsec.",
        placeholder: "ncryptsec…",
        secret: false,
        shown: true,
        draft,
        submitLabel: "OK",
        ready: true,
        busy: false
      };
    }
    if (identity._tag === "Signer" && identity.editing !== undefined) {
      return {
        place: "SIGNER",
        prompt: "Signer address",
        fieldLabel: "Signer address",
        note: "Your signer app shows it. It starts with bunker://.",
        placeholder: "bunker://…",
        secret: false,
        shown: true,
        draft: identity.editing,
        submitLabel: "OK",
        ready: true,
        busy: false
      };
    }
    return;
  }
  function nameEntry(nav, korri) {
    const draft = nav.name.draft ?? picoTextDraft(deviceName(korri) ?? "");
    const maxLength = deviceNameSetting(korri)?.maxLength;
    const problem = settingProblem(korri, DEVICE_NAME_SETTING);
    return {
      place: "NAME",
      prompt: "Name this device",
      fieldLabel: "Device name",
      note: "Your other Korri devices show this name.",
      placeholder: UNNAMED_DEVICE,
      ...maxLength === undefined ? {} : { maxLength },
      secret: false,
      shown: true,
      draft,
      submitLabel: "NEXT",
      ready: draft.text.trim() !== "" && nav.name.saving === "No",
      busy: nav.name.saving !== "No",
      ...problem === undefined ? {} : { problem }
    };
  }
  function personEntry(nav, korri) {
    const draft = nav.person.draft ?? picoTextDraft(personName(korri) ?? "");
    const empty = draft.text.trim() === "";
    const maxLength = personNameSetting(korri)?.maxLength;
    const problem = settingProblem(korri, PERSON_NAME_SETTING);
    return {
      place: "YOUR NAME",
      prompt: "What is your name?",
      fieldLabel: "Your name",
      note: "Your devices show it when you play. You can leave it empty.",
      placeholder: "Your name",
      ...maxLength === undefined ? {} : { maxLength },
      secret: false,
      shown: true,
      draft,
      submitLabel: empty ? "SKIP" : "NEXT",
      ready: nav.person.saving === "No",
      busy: nav.person.saving !== "No",
      ...problem === undefined ? {} : { problem }
    };
  }
  function padsView(korri) {
    const controllers = korri.onboarding?.controllers ?? [];
    const seats = picoSettingsViewFromModel(korri).groups.flatMap((group) => group.rows).find((row) => row.id === PLAYER_SEATS_SETTING && (row.control.kind === "step" || row.control.kind === "cycle"));
    return {
      controllers,
      ...seats === undefined ? {} : { seats },
      ready: controllers.length === 0 || controllers.some((controller) => controller.heard)
    };
  }
  var gamesScanning = (korri) => korri.catalog._tag === "Loading" || settingSaving(korri, GAME_RESCAN_ACTION);
  var gamesCanContinue = (korri) => !gamesScanning(korri) || korri.onboarding?.starterPack?._tag === "Installing";
  function gamesView(korri) {
    const scanning = { _tag: "Scanning", canContinue: gamesCanContinue(korri) };
    if (gamesScanning(korri))
      return scanning;
    const canScan = canRescanGames(korri);
    const problem = settingProblem(korri, GAME_RESCAN_ACTION);
    const more = problem === undefined ? {} : { problem };
    const home = picoHomeViewFromCatalog(korri.catalog);
    switch (home._tag) {
      case "Loading":
        return scanning;
      case "Empty":
        return { _tag: "Empty", canScan, ...more };
      case "Failed":
        return { _tag: "Failed", message: home.message, canScan, ...more };
      case "Shelf":
        return { _tag: "Found", games: home.games, canScan, ...more };
    }
  }
  function starterPackChoices(view) {
    switch (view._tag) {
      case "Offered":
      case "Failed":
        return view.actions;
      case "None":
      case "Installing":
      case "Installed":
        return [];
    }
  }
  function starterPackAction(action, index, first) {
    return { id: action.id, label: index === 0 ? first : action.label, enabled: action.enabled };
  }
  function starterPackView(korri) {
    const pack = korri.onboarding?.starterPack;
    switch (pack?._tag) {
      case undefined:
        return { _tag: "None" };
      case "Offered":
        return { _tag: "Offered", actions: pack.actions.map((action, index) => starterPackAction(action, index, "Install Starter pack")) };
      case "Installing":
      case "Installed":
        return { _tag: pack._tag };
      case "Failed":
        return {
          _tag: "Failed",
          message: pack.message,
          actions: pack.actions.map((action, index) => starterPackAction(action, index, "Try Starter pack again"))
        };
    }
  }
  var ORIGIN_FACT = { created: "New key", restored: "Restored", signer: "Signer" };
  function facts(nav, korri) {
    const network = korri.network;
    const name = deviceName(korri);
    const heard = (korri.onboarding?.controllers ?? []).filter((controller) => controller.heard).length;
    const seats = settingItem(korri, PLAYER_SEATS_SETTING)?.value;
    const catalog = korri.catalog;
    const games = catalog._tag === "Ready" ? catalog.games.length === 0 ? "None yet" : `${catalog.games.length} found` : catalog._tag === "Empty" ? "None yet" : catalog._tag === "Loading" ? "Still looking" : "Not read";
    return [
      ...network === undefined ? [] : [{ label: "Network", value: picoConnectionLabel(network.connection) }],
      ...name === undefined ? [] : [{ label: "Name", value: name }],
      ...nav.identity._tag === "Ready" ? [{ label: "Identity", value: ORIGIN_FACT[nav.identity.origin] }] : [],
      { label: "Controllers", value: `${heard} ready` },
      ...seats === undefined ? [] : [{ label: "Player seats", value: seats }],
      { label: "Games", value: games }
    ];
  }
  function picoOnboardingScreen(nav, korri) {
    switch (nav.step) {
      case "hello":
        return { _tag: "Hello", askWifi: asksForWifi(korri) };
      case "wifi": {
        const question = nav.wifi.question;
        if (question._tag === "Entry")
          return { _tag: "Entry", entry: { ...picoWifiEntryView(question.entry), busy: false } };
        return { _tag: "Wifi", view: picoWifiView(nav.wifi, korri) };
      }
      case "name":
        return { _tag: "Entry", entry: nameEntry(nav, korri) };
      case "person":
        return { _tag: "Entry", entry: personEntry(nav, korri) };
      case "you": {
        const entry = identityEntry(nav.identity);
        if (entry !== undefined)
          return { _tag: "Entry", entry };
        const view = identityView(nav.identity, korri);
        const backup = nav.backup;
        if (backup === undefined)
          return { _tag: "Identity", view };
        const identity = korri.identityManagement;
        return {
          _tag: "Identity",
          view,
          backup: {
            password: backup.password,
            ...identity === undefined ? {} : { identity },
            ...backup.qr?.dataUrl === undefined ? {} : { qr: backup.qr.dataUrl }
          }
        };
      }
      case "pads":
        return { _tag: "Pads", view: padsView(korri) };
      case "games":
        return { _tag: "Games", view: gamesView(korri), starterPack: starterPackView(korri) };
      case "play":
        return { _tag: "Play", facts: facts(nav, korri), finishing: nav.finishing };
    }
  }
  function picoSetupTrack(nav, korri) {
    const shown = nav.step === "person" ? "you" : nav.step;
    const at = ONBOARDING_STEPS.findIndex((step) => step.id === shown);
    const wifiSkipped = korri.network === undefined || nav.continuedOffline && isOffline(korri);
    return ONBOARDING_STEPS.map((step, index) => ({
      label: step.label,
      state: index === at ? "now" : index > at ? "next" : step.id === "wifi" && wifiSkipped ? "skipped" : "done"
    }));
  }
  function picoSetupPlace(step, screen) {
    if (screen._tag === "Entry")
      return screen.entry.place;
    return ONBOARDING_STEPS.find((candidate) => candidate.id === step)?.label ?? "";
  }
  function picoSetupCursor(step, screen) {
    switch (screen._tag) {
      case "Hello":
        return { key: "hello", at: "first" };
      case "Wifi":
        return { key: `wifi:${screen.view._tag}`, at: screen.view._tag === "OfflineWarning" ? "last" : "first" };
      case "Entry":
        return { key: `${step}:entry:${screen.entry.place}:${screen.entry.prompt}`, at: "first" };
      case "Identity":
        return { key: `you:${screen.view._tag}`, at: "first" };
      case "Pads":
        return { key: `pads:${String(screen.view.ready)}`, at: "last" };
      case "Games":
        return { key: `games:${screen.starterPack._tag}:${screen.view._tag}`, at: "first" };
      case "Play":
        return { key: "play", at: "last" };
    }
  }

  // src/state/requests.ts
  var stay = (model) => ({ model, requests: [] });
  var ask = (model, ...requests) => ({ model, requests });

  // src/state/onboarding-update.ts
  function updateOnboarding(nav, message, korri) {
    const result = step(nav, message, korri);
    const settled = settle(result.model, korri);
    return { model: settled.model, requests: [...result.requests, ...settled.requests] };
  }
  function step(nav, message, korri) {
    switch (message._tag) {
      case "KorriPublished":
      case "Waited":
        return stay(nav);
      case "RenderedQr": {
        const backup = nav.backup;
        return backup?.qr?.text === message.text ? stay({ ...nav, backup: { ...backup, qr: { text: message.text, dataUrl: message.dataUrl } } }) : stay(nav);
      }
      case "PressedBack":
        return back(nav, korri);
      case "Started":
        return nav.step === "hello" ? enterWifi(nav, korri) : stay(nav);
      case "Continued":
        return continued(nav, korri);
      case "PickedNetwork":
        return nav.step === "wifi" ? join(nav, korri, pickNetwork(korri, message.networkId)) : stay(nav);
      case "ChoseHiddenNetwork":
        return nav.step === "wifi" ? stay(withWifi(nav, { question: { _tag: "Entry", entry: HIDDEN_NAME_ENTRY } })) : stay(nav);
      case "ScannedNetworks":
        return nav.step === "wifi" ? scan(nav, korri) : stay(nav);
      case "TriedAgain":
        return nav.step === "wifi" ? join(seeFailure(nav, korri), korri, retryAttempt(korri)) : stay(nav);
      case "PickedAnotherNetwork":
      case "ReturnedToWifi":
        return nav.step === "wifi" ? stay(withWifi(seeFailure(nav, korri), { showing: "List", question: { _tag: "None" } })) : stay(nav);
      case "StayedConnected":
        return nav.step === "wifi" ? stay(withWifi(seeFailure(nav, korri), { showing: "Auto" })) : stay(nav);
      case "ChoseWifiToo":
        return nav.step === "wifi" ? scan(withWifi(nav, { showing: "List" }), korri) : stay(nav);
      case "ChoseOffline": {
        const view = picoWifiView(nav.wifi, korri);
        return nav.step === "wifi" && isOffline(korri) && (view._tag === "NoNetworks" || view._tag === "Failed" && view.fallback._tag === "Offline") ? stay(withWifi(nav, { question: { _tag: "Warning" } })) : stay(nav);
      }
      case "ConfirmedOffline": {
        const view = picoWifiView(nav.wifi, korri);
        if (nav.step !== "wifi" || view._tag !== "OfflineWarning")
          return stay(nav);
        return stay(enterName({ ...withWifi(nav, { question: { _tag: "None" } }), continuedOffline: true }, korri));
      }
      case "TypedText":
        return stay(editText(nav, korri, (draft, maxLength) => picoDraftType(draft, message.character, maxLength)));
      case "PressedTextBackspace":
        return stay(editText(nav, korri, (draft) => picoDraftBackspace(draft)));
      case "ClearedText":
        return stay(editText(nav, korri, (draft) => ({ ...draft, text: "" })));
      case "ToggledCapitals":
        return stay(editText(nav, korri, (draft) => ({ ...draft, capitals: !draft.capitals })));
      case "ToggledSymbols":
        return stay(editText(nav, korri, (draft) => ({ ...draft, symbols: !draft.symbols })));
      case "ToggledShown":
        return stay(toggleSecret(nav));
      case "SubmittedText":
        return submitText(nav, korri);
      case "DismissedSettingsProblem":
        return ask(nav, { _tag: "DismissSettingsProblem" });
      case "ChoseIdentity":
        return chooseIdentity(nav, korri, message.how);
      case "EditedIdentityField":
        return editIdentityField(nav, message.field);
      case "ToggledTrustLoss":
        return toggleTrustLoss(nav);
      case "SubmittedIdentity":
        return submitIdentity(nav, korri);
      case "OpenedBackup":
        return nav.step === "you" && nav.identity._tag === "Ready" ? ask({ ...nav, backup: { password: "", qr: undefined } }, { _tag: "DismissIdentityStatus" }) : stay(nav);
      case "EditedBackupPassword":
        return nav.backup === undefined ? stay(nav) : stay({ ...nav, backup: { ...nav.backup, password: message.password } });
      case "SubmittedBackup":
        return nav.backup === undefined || nav.backup.password.trim() === "" || korri.identityManagement?.status._tag === "Working" ? stay(nav) : ask(nav, { _tag: "ExportIdentityBackup", password: nav.backup.password });
      case "ClosedBackup":
        return closeBackup(nav);
      case "ChangedSetting":
        return nav.step === "pads" ? ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value }) : stay(nav);
      case "ScannedGames":
        return nav.step === "games" && canRescanGames(korri) && !gamesScanning(korri) ? ask(nav, { _tag: "RunAction", actionId: GAME_RESCAN_ACTION }) : stay(nav);
      case "ChoseStarterPackAction":
        return nav.step === "games" && starterPackChoices(starterPackView(korri)).some((action) => action.id === message.actionId && action.enabled) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "Finished":
        return nav.step === "play" && !nav.finishing ? ask({ ...nav, finishing: true }, { _tag: "FinishOnboarding" }) : stay(nav);
    }
  }
  var at = (nav, step2) => ({ ...nav, step: step2 });
  function enterWifi(nav, korri) {
    if (korri.network === undefined)
      return stay(enterName(nav, korri));
    const next = { ...at(nav, "wifi"), wifi: { ...nav.wifi, showing: "Auto", question: { _tag: "None" } } };
    const wifi = korri.network.wifi;
    return wifi._tag === "Available" && korri.network.connection._tag !== "Wired" ? scan(next, korri) : stay(next);
  }
  function enterName(nav, korri) {
    const draft = nav.name.draft ?? picoTextDraft(deviceName(korri) ?? "");
    return { ...at(nav, "name"), name: { draft, saving: "No" } };
  }
  function continued(nav, korri) {
    switch (nav.step) {
      case "wifi": {
        const view = picoWifiView(nav.wifi, korri);
        if (!wifiDone(view))
          return stay(nav);
        return stay(enterName({ ...nav, continuedOffline: nav.continuedOffline || isOffline(korri) }, korri));
      }
      case "you":
        return nav.identity._tag === "Ready" && nav.backup === undefined ? stay(afterIdentity(nav, korri)) : stay(nav);
      case "pads":
        return padsView(korri).ready ? stay(at(nav, "games")) : stay(nav);
      case "games":
        return gamesCanContinue(korri) ? stay(at(nav, "play")) : stay(nav);
      case "hello":
      case "name":
      case "person":
      case "play":
        return stay(nav);
    }
  }
  function afterIdentity(nav, korri) {
    return personNameSetting(korri) === undefined ? at(nav, "pads") : enterPerson(nav, korri);
  }
  function enterPerson(nav, korri) {
    const draft = nav.person.draft ?? picoTextDraft(personName(korri) ?? "");
    return { ...at(nav, "person"), person: { draft, saving: "No" } };
  }
  function back(nav, korri) {
    switch (nav.step) {
      case "hello":
        return stay(nav);
      case "wifi":
        return stay(backInWifi(nav, korri));
      case "name":
        return stay(korri.network === undefined ? at(nav, "hello") : at(nav, "wifi"));
      case "you":
        return backInIdentity(nav, korri);
      case "person":
        return stay(at(nav, "you"));
      case "pads":
        return stay(personNameSetting(korri) === undefined ? at(nav, "you") : enterPerson(nav, korri));
      case "games":
        return stay(at(nav, "pads"));
      case "play":
        return nav.finishing ? stay(nav) : stay(at(nav, "games"));
    }
  }
  function backInWifi(nav, korri) {
    const wifi = nav.wifi;
    if (wifi.question._tag !== "None")
      return withWifi(nav, { question: { _tag: "None" } });
    const connection = korri.network?.connection;
    if (wifi.showing === "List" && connection !== undefined && connection._tag !== "Offline") {
      return withWifi(nav, { showing: "Auto" });
    }
    return { ...at(nav, "hello"), wifi: { ...WIFI_STEP, seenFailure: wifi.seenFailure } };
  }
  function backInIdentity(nav, korri) {
    if (nav.backup !== undefined)
      return closeBackup(nav);
    const identity = nav.identity;
    switch (identity._tag) {
      case "Restore":
      case "Signer":
        if (identity.editing !== undefined)
          return stay(withIdentity(nav, { ...identity, editing: undefined }));
        if (identity.sent && korri.identityManagement?.status._tag === "Working")
          return stay(nav);
        return ask(withIdentity(nav, { _tag: "Choose" }), { _tag: "DismissIdentityStatus" });
      case "Choose":
      case "Ready":
        return stay(at(nav, "name"));
    }
  }
  var withWifi = (nav, change) => ({ ...nav, wifi: { ...nav.wifi, ...change } });
  function scan(nav, korri) {
    const wifi = korri.network?.wifi;
    if (wifi?._tag !== "Available")
      return stay(nav);
    if (wifi.scanning)
      return stay(nav);
    return ask(withWifi(nav, { pending: { _tag: "Scan", saw: false, before: wifi.networks } }), { _tag: "ScanWifi" });
  }
  function seeFailure(nav, korri) {
    const attempt = lastAttempt(korri);
    return attempt._tag === "Failed" ? withWifi(nav, { seenFailure: attempt }) : nav;
  }
  function join(nav, korri, next) {
    switch (next._tag) {
      case "Edit":
        return stay(withWifi(nav, { question: { _tag: "Entry", entry: next.entry } }));
      case "Send": {
        const ssid = joiningSsid(next.request, korri);
        return ask(withWifi(nav, {
          showing: "Auto",
          question: { _tag: "None" },
          pending: ssid === undefined ? undefined : { _tag: "Join", ssid, saw: false, before: lastAttempt(korri) }
        }), next.request);
      }
      case "Nothing":
        return stay(nav);
    }
  }
  function joiningSsid(request, korri) {
    if (request._tag === "ConnectHiddenWifi")
      return request.ssid;
    if (request._tag === "ConnectWifi")
      return scannedNetworks(korri).find((network) => network.id === request.networkId)?.ssid;
    return;
  }
  function editText(nav, korri, change) {
    switch (nav.step) {
      case "wifi": {
        const question = nav.wifi.question;
        if (question._tag !== "Entry")
          return nav;
        const entry = question.entry;
        return withWifi(nav, { question: { _tag: "Entry", entry: editEntry(entry, (draft) => change(draft, entryMaxLength(entry))) } });
      }
      case "name": {
        if (nav.name.saving !== "No" || nav.name.draft === undefined)
          return nav;
        return { ...nav, name: { ...nav.name, draft: change(nav.name.draft, deviceNameSetting(korri)?.maxLength) } };
      }
      case "person": {
        if (nav.person.saving !== "No" || nav.person.draft === undefined)
          return nav;
        return { ...nav, person: { ...nav.person, draft: change(nav.person.draft, personNameSetting(korri)?.maxLength) } };
      }
      case "you": {
        const identity = nav.identity;
        if (identity._tag === "Restore" && identity.editing !== undefined) {
          return withIdentity(nav, { ...identity, editing: { ...identity.editing, draft: change(identity.editing.draft, undefined) } });
        }
        if (identity._tag === "Signer" && identity.editing !== undefined) {
          return withIdentity(nav, { ...identity, editing: change(identity.editing, undefined) });
        }
        return nav;
      }
      case "hello":
      case "pads":
      case "games":
      case "play":
        return nav;
    }
  }
  function toggleSecret(nav) {
    if (nav.step === "wifi" && nav.wifi.question._tag === "Entry") {
      return withWifi(nav, { question: { _tag: "Entry", entry: toggleShown(nav.wifi.question.entry) } });
    }
    const identity = nav.identity;
    if (nav.step === "you" && identity._tag === "Restore" && identity.editing?.field === "password") {
      return withIdentity(nav, { ...identity, editing: { ...identity.editing, shown: !identity.editing.shown } });
    }
    return nav;
  }
  function submitText(nav, korri) {
    switch (nav.step) {
      case "wifi": {
        const question = nav.wifi.question;
        return question._tag === "Entry" ? join(nav, korri, submitEntry(question.entry)) : stay(nav);
      }
      case "name":
        return submitName(nav, korri);
      case "person":
        return submitPerson(nav, korri);
      case "you": {
        const identity = nav.identity;
        if (identity._tag === "Restore" && identity.editing !== undefined) {
          const { field, draft } = identity.editing;
          return stay(withIdentity(nav, field === "backup" ? { ...identity, backup: draft.text.trim(), editing: undefined } : { ...identity, password: draft.text, editing: undefined }));
        }
        if (identity._tag === "Signer" && identity.editing !== undefined) {
          return stay(withIdentity(nav, { ...identity, uri: identity.editing.text.trim(), editing: undefined }));
        }
        return stay(nav);
      }
      case "hello":
      case "pads":
      case "games":
      case "play":
        return stay(nav);
    }
  }
  function submitName(nav, korri) {
    const text = nav.name.draft?.text ?? "";
    if (text.trim() === "" || nav.name.saving !== "No")
      return stay(nav);
    if (deviceNameSetting(korri) === undefined || text === deviceName(korri))
      return stay(at(nav, "you"));
    return ask({ ...nav, name: { ...nav.name, saving: "Sent" } }, { _tag: "ChangeSetting", settingId: DEVICE_NAME_SETTING, value: text });
  }
  function submitPerson(nav, korri) {
    const text = nav.person.draft?.text ?? "";
    if (nav.person.saving !== "No")
      return stay(nav);
    if (text.trim() === "" || personNameSetting(korri) === undefined || text === personName(korri))
      return stay(at(nav, "pads"));
    return ask({ ...nav, person: { ...nav.person, saving: "Sent" } }, { _tag: "ChangeSetting", settingId: PERSON_NAME_SETTING, value: text });
  }
  var withIdentity = (nav, identity) => ({ ...nav, identity });
  function chooseIdentity(nav, korri, how) {
    if (nav.step !== "you" || nav.identity._tag !== "Choose")
      return stay(nav);
    if (how === "created")
      return stay(withIdentity(nav, { _tag: "Ready", origin: "created" }));
    if (korri.identityManagement === undefined || isOffline(korri))
      return stay(nav);
    return ask(withIdentity(nav, how === "restored" ? { _tag: "Restore", backup: "", password: "", editing: undefined, confirmed: false, sent: false } : { _tag: "Signer", uri: "", editing: undefined, confirmed: false, sent: false }), { _tag: "DismissIdentityStatus" });
  }
  function editIdentityField(nav, field) {
    const identity = nav.identity;
    const cleared = "sent" in identity && identity.sent ? [{ _tag: "DismissIdentityStatus" }] : [];
    if (identity._tag === "Restore" && (field === "backup" || field === "password")) {
      return ask(withIdentity(nav, {
        ...identity,
        sent: false,
        editing: { field, draft: picoTextDraft(identity[field]), shown: false }
      }), ...cleared);
    }
    if (identity._tag === "Signer" && field === "uri") {
      return ask(withIdentity(nav, { ...identity, sent: false, editing: picoTextDraft(identity.uri) }), ...cleared);
    }
    return stay(nav);
  }
  function toggleTrustLoss(nav) {
    const identity = nav.identity;
    if (nav.step !== "you" || identity._tag !== "Restore" && identity._tag !== "Signer" || identity.editing !== undefined || identity.sent)
      return stay(nav);
    return stay(withIdentity(nav, { ...identity, confirmed: !identity.confirmed }));
  }
  function submitIdentity(nav, korri) {
    const identity = nav.identity;
    if (korri.identityManagement === undefined || korri.identityManagement.status._tag === "Working")
      return stay(nav);
    if (identity._tag === "Restore" && identity.editing === undefined && identity.confirmed && identity.backup.trim() !== "" && identity.password !== "") {
      return ask(withIdentity(nav, { ...identity, password: "", sent: true }), {
        _tag: "SwitchIdentityFromBackup",
        encryptedSecret: identity.backup.trim(),
        password: identity.password,
        disposition: "transfer",
        trustLossConfirmed: identity.confirmed
      });
    }
    if (identity._tag === "Signer" && identity.editing === undefined && identity.confirmed && identity.uri !== "") {
      return ask(withIdentity(nav, { ...identity, sent: true }), {
        _tag: "SwitchIdentityToNip46",
        bunkerUri: identity.uri,
        disposition: "transfer",
        trustLossConfirmed: identity.confirmed
      });
    }
    return stay(nav);
  }
  var closeBackup = (nav) => nav.backup === undefined ? stay(nav) : ask({ ...nav, backup: undefined }, { _tag: "DismissIdentityStatus" });
  function settle(nav, korri) {
    let next = settlePending(nav, korri);
    next = settleName(next, korri);
    next = settlePerson(next, korri);
    next = settleIdentity(next, korri);
    if (next.step === "wifi" && korri.network === undefined)
      next = enterName(next, korri);
    return settleBackup(next, korri);
  }
  function settlePending(nav, korri) {
    const pending = nav.wifi.pending;
    if (pending === undefined)
      return nav;
    const wifi = korri.network?.wifi;
    if (wifi?._tag !== "Available")
      return withWifi(nav, { pending: undefined });
    switch (pending._tag) {
      case "Scan":
        if (wifi.scanning)
          return pending.saw ? nav : withWifi(nav, { pending: { ...pending, saw: true } });
        return pending.saw || !sameNetworks(wifi.networks, pending.before) ? withWifi(nav, { pending: undefined }) : nav;
      case "Join": {
        if (onNetwork(korri, pending.ssid))
          return withWifi(nav, { pending: undefined });
        if (wifi.attempt._tag === "Connecting")
          return pending.saw ? nav : withWifi(nav, { pending: { ...pending, saw: true } });
        return pending.saw || !sameAttempt(wifi.attempt, pending.before) ? withWifi(nav, { pending: undefined }) : nav;
      }
    }
  }
  var sameNetworks = (a, b) => a.length === b.length && a.every((network, index) => network.id === b[index]?.id);
  function settleSave(name, settingId, held, korri) {
    if (name.saving === "No")
      return name;
    const status = korri.settingsStatus;
    const mine = status._tag !== "Idle" && status.settingId === settingId;
    if (mine && status._tag === "Saving")
      return name.saving === "Seen" ? name : { ...name, saving: "Seen" };
    if (mine && status._tag === "Problem")
      return { ...name, saving: "No" };
    return name.saving === "Seen" || held === name.draft?.text ? "Saved" : name;
  }
  function settleName(nav, korri) {
    const name = settleSave(nav.name, DEVICE_NAME_SETTING, deviceName(korri), korri);
    if (name === "Saved")
      return { ...at(nav, nav.step === "name" ? "you" : nav.step), name: { ...nav.name, saving: "No" } };
    return name === nav.name ? nav : { ...nav, name };
  }
  function settlePerson(nav, korri) {
    const person = settleSave(nav.person, PERSON_NAME_SETTING, personName(korri), korri);
    if (person === "Saved")
      return { ...at(nav, nav.step === "person" ? "pads" : nav.step), person: { ...nav.person, saving: "No" } };
    return person === nav.person ? nav : { ...nav, person };
  }
  function settleIdentity(nav, korri) {
    const identity = nav.identity;
    if (identity._tag !== "Restore" && identity._tag !== "Signer" || !identity.sent)
      return nav;
    if (korri.identityManagement?.status._tag !== "Switched")
      return nav;
    return withIdentity(nav, { _tag: "Ready", origin: identity._tag === "Restore" ? "restored" : "signer" });
  }
  function settleBackup(nav, korri) {
    const backup = nav.backup;
    if (backup === undefined)
      return stay(nav);
    const status = korri.identityManagement?.status;
    const text = status?._tag === "BackupReady" ? status.encryptedSecret : undefined;
    if (text === backup.qr?.text)
      return stay(nav);
    if (text === undefined)
      return stay({ ...nav, backup: { ...backup, qr: undefined } });
    return ask({ ...nav, backup: { ...backup, qr: { text, dataUrl: undefined } } }, { _tag: "RenderQr", text });
  }

  // src/state/overlay.ts
  var RANGE_COMMIT_DELAY_MS = 180;
  var RANGE_RELEASE_FALLBACK_MS = 2000;
  var initialOverlay = { asking: undefined, ranges: {}, nextToken: 0 };
  var invoke = (control) => ({ _tag: "InvokeGameplayControl", controlId: control.id, value: control.sends });
  function updateOverlay(state, message, overlay) {
    const result = step2(state, message);
    const ranges = settleRanges(result.model.ranges, overlay);
    return { model: ranges === result.model.ranges ? result.model : { ...result.model, ranges }, requests: result.requests };
  }
  function step2(state, message) {
    switch (message._tag) {
      case "PressedBack":
        return state.asking !== undefined ? stay({ ...state, asking: undefined }) : dismiss(state);
      case "PressedMenu":
      case "PressedSystem":
        return dismiss(state);
      case "PressedOptions":
      case "KorriPublished":
      case "RenderedQr":
        return stay(state);
      case "AskedControl":
        return stay({ ...state, asking: message.control });
      case "CancelledControl":
        return stay({ ...state, asking: undefined });
      case "ConfirmedControl":
        return state.asking === undefined ? stay(state) : ask({ ...state, asking: undefined }, invoke(state.asking));
      case "InvokedControl":
        return ask(state, invoke(message.control));
      case "PressedRetry":
        return ask(state, { _tag: "Retry" });
      case "SteppedRange": {
        const { control, request } = message;
        const range = control.range;
        if (!control.enabled || range === undefined)
          return stay(state);
        const edit = state.ranges[control.id];
        const current = edit?.value ?? range.value;
        const next = picoRangeStep({ ...range, value: current }, request.way);
        if (next === current)
          return stay(state);
        const token = state.nextToken;
        return ask({
          ...state,
          nextToken: token + 1,
          ranges: {
            ...state.ranges,
            [control.id]: { value: next, base: range, enabled: control.enabled, pending: { request, token } }
          }
        }, { _tag: "Wait", ms: request.releaseExpected ? RANGE_RELEASE_FALLBACK_MS : RANGE_COMMIT_DELAY_MS, token });
      }
      case "ReleasedRange": {
        const pending = state.ranges[message.control.id]?.pending?.request;
        const ended = message.ended;
        if (pending === undefined || pending.way !== ended.way || pending.gestureId !== ended.gestureId || pending.source !== ended.source)
          return stay(state);
        return flush(state, [message.control.id]);
      }
      case "LeftRange":
        return flush(state, [message.control.id]);
      case "Waited":
        return flush(state, Object.keys(state.ranges).filter((id) => state.ranges[id]?.pending?.token === message.token));
    }
  }
  function dismiss(state) {
    const sent = flush(state, Object.keys(state.ranges));
    return ask(sent.model, ...sent.requests, { _tag: "DismissGameplayOverlay" });
  }
  function flush(state, ids) {
    const requests = [];
    let ranges = state.ranges;
    for (const id of ids) {
      const edit = ranges[id];
      if (edit?.pending === undefined)
        continue;
      ranges = { ...ranges, [id]: { ...edit, pending: undefined } };
      requests.push({ _tag: "InvokeGameplayControl", controlId: id, value: { kind: "range", value: edit.value } });
    }
    return requests.length === 0 ? stay(state) : ask({ ...state, ranges }, ...requests);
  }
  function settleRanges(ranges, overlay) {
    const ids = Object.keys(ranges);
    if (ids.length === 0)
      return ranges;
    const controls = [...overlay.controls, ...overlay.groups.flatMap((group) => group.controls)];
    const kept = ids.filter((id) => {
      const edit = ranges[id];
      const control = controls.find((candidate) => candidate.id === id);
      return edit !== undefined && control?.range !== undefined && control.enabled === edit.enabled && sameRange(control.range, edit.base);
    });
    return kept.length === ids.length ? ranges : Object.fromEntries(kept.map((id) => [id, ranges[id]]));
  }
  var sameRange = (a, b) => a.value === b.value && a.min === b.min && a.max === b.max && a.step === b.step;
  var rangeValue = (state, control) => state.ranges[control.id]?.value ?? control.range?.value;

  // src/pico-screen-view.ts
  function picoScreenViewFromModel(model) {
    const status = model.status;
    switch (status._tag) {
      case "Busy": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Busy",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          ...status.progress === undefined ? {} : { progress: status.progress },
          ...status.actions === undefined ? {} : { actions: status.actions },
          ...cart === undefined ? {} : { cart }
        };
      }
      case "Confirm": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Confirm",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          actions: status.actions,
          ...status.declineActionId === undefined ? {} : { declineActionId: status.declineActionId },
          ...cart === undefined ? {} : { cart }
        };
      }
      case "ChooseSave": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Confirm",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          actions: picoChooseSaveActions(status),
          ...cart === undefined ? {} : { cart }
        };
      }
      case "Running": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Running",
          kicker: status.kicker,
          ...cart === undefined ? {} : { gameTitle: cart.title, cart }
        };
      }
      case "Problem":
        return {
          _tag: "Problem",
          kicker: status.kicker,
          reason: status.reason,
          canRetry: status.canRetry,
          ...status.gameTitle === undefined ? {} : { gameTitle: status.gameTitle }
        };
      case "Paused": {
        const cart = cartFor(model, status.gameId);
        return {
          _tag: "Paused",
          kicker: status.kicker,
          ...status.detail === undefined ? {} : { detail: status.detail },
          actions: status.actions,
          ...cart === undefined ? {} : { cart }
        };
      }
      case "Ended":
        return { _tag: "Ended", kicker: status.kicker, reason: status.reason };
      case "Browsing":
        return picoHomeViewFromCatalog(model.catalog);
    }
  }
  var MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
  var two = (n) => String(n).padStart(2, "0");
  function picoSaveTime(at2) {
    const date = new Date(at2);
    if (Number.isNaN(date.getTime()))
      return "--";
    return `${MONTHS[date.getMonth()] ?? ""} ${date.getDate()} ${two(date.getHours())}:${two(date.getMinutes())}`;
  }
  function picoChooseSaveActions(status) {
    return [
      ...status.sides.map((side) => ({
        id: side.actionId,
        label: `${side.device}, ${picoSaveTime(side.endedAt)}`,
        enabled: true
      })),
      ...status.actions
    ];
  }
  function cartFor(model, gameId) {
    if (gameId === undefined)
      return;
    if (model.catalog._tag !== "Ready")
      return;
    const game = model.catalog.games.find((candidate) => candidate.id === gameId);
    if (game === undefined)
      return;
    return {
      id: game.id,
      title: game.title,
      ...game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl }
    };
  }

  // src/state/shown.ts
  function shownScreen(nav, korri) {
    const view = picoScreenViewFromModel(korri);
    const browsing = korri.status._tag === "Browsing";
    const game = nav.detail !== undefined && view._tag === "Shelf" && korri.catalog._tag === "Ready" ? korri.catalog.games.find((candidate) => candidate.id === nav.detail?.gameId) : undefined;
    if (nav.settings !== undefined && browsing)
      return { _tag: "Settings", settings: nav.settings };
    if (nav.store && browsing && game === undefined && korri.store !== undefined) {
      return { _tag: "Store", place: nav.storePlace, store: korri.store };
    }
    if (nav.find && browsing && game === undefined)
      return { _tag: "Find", search: nav.search };
    if (nav.detail !== undefined && game !== undefined)
      return { _tag: "Detail", detail: nav.detail, game };
    return { _tag: "Home", home: nav.home, view };
  }
  var runnerOpen = (korri) => korri.runnerChoice !== undefined && korri.runnerChoice._tag !== "Closed";
  var launchOpen = picoLaunchOpen;
  function canAttract(nav, korri) {
    const shown = shownScreen(nav, korri);
    return !runnerOpen(korri) && !launchOpen(korri) && shown._tag === "Home" && shown.view._tag === "Shelf" && nav.home.menu === "Closed" && nav.detail === undefined;
  }
  var invitesNoticeable = (korri) => korri.presentation.kind === "catalog" && korri.onboarding === undefined;
  function inviteNotices(nav, korri) {
    if (!invitesNoticeable(korri))
      return [];
    const invites = korri.invites ?? [];
    return nav.invites.showing.flatMap(({ inviteId }) => {
      const invite = invites.find((candidate) => candidate.id === inviteId);
      return invite === undefined ? [] : [{
        id: invite.id,
        personName: invite.personName,
        gameTitle: invite.gameTitle,
        deviceLabel: invite.deviceLabel
      }];
    });
  }
  var attractShowing = (nav, korri) => nav.idle._tag === "Attracting" && canAttract(nav, korri);
  var identityAction = (nav) => nav.settings?.question._tag === "Identity" ? nav.settings.question.actionId : null;

  // src/pico-invite-view.ts
  var PICO_INVITE_NOTICE_MS = 8000;
  var picoInvitePlaying = (personName2) => `${personName2} is playing`;
  var picoInviteWhere = (deviceLabel) => `on ${deviceLabel}`;
  function picoInvitesFrom(model) {
    return (model.invites ?? []).map((invite) => ({
      id: invite.id,
      personName: invite.personName,
      gameTitle: invite.gameTitle,
      deviceLabel: invite.deviceLabel,
      ...invite.coverArtUrl === undefined ? {} : { artUrl: invite.coverArtUrl },
      actions: invite.actions
    }));
  }
  function picoInviteActionPublished(model, actionId) {
    return (model.invites ?? []).some((invite) => invite.actions.some((action) => action.id === actionId && action.enabled));
  }

  // src/pico-session-return.ts
  function picoSessionReturnOnPlay(previous, gameId, catalog) {
    if (previous._tag === "Watching" && previous.viewingId === gameId)
      return previous;
    if (catalog._tag !== "Ready")
      return { _tag: "Idle" };
    return catalog.games.some((game) => game.id === gameId && game.resumable === true) ? { _tag: "Watching", viewingId: gameId, sessionId: gameId } : { _tag: "AwaitingSession", viewingId: gameId, catalog };
  }
  function picoSessionReturnFromModel(previous, viewingId, model) {
    const idle = { _tag: "Idle" };
    if (viewingId === undefined)
      return idle;
    const state = "viewingId" in previous && previous.viewingId === viewingId ? previous : idle;
    const { catalog, status } = model;
    if (status._tag === "Problem" && state._tag === "AwaitingSession")
      return idle;
    if (catalog._tag !== "Ready" || status._tag !== "Browsing")
      return state;
    const resumable = catalog.games.filter((game) => game.resumable === true);
    if (state._tag === "Watching") {
      if (catalog.games.some((game) => game.id === state.sessionId))
        return state;
      return resumable.length === 0 ? { _tag: "ReturnToLibrary" } : idle;
    }
    if (state._tag === "AwaitingSession") {
      if (catalog === state.catalog)
        return state;
      const added = resumable.filter((game) => !state.catalog.games.some((old) => old.id === game.id));
      const session = added.length === 1 ? added[0] : undefined;
      return session === undefined ? idle : { _tag: "Watching", viewingId, sessionId: session.id };
    }
    return resumable.some((game) => game.id === viewingId) ? { _tag: "Watching", viewingId, sessionId: viewingId } : idle;
  }

  // src/pico-store-view.ts
  var picoStoreResult = (entry) => ({
    id: entry.id,
    title: entry.title,
    ...entry.system === undefined ? {} : { subtitle: entry.system }
  });
  var PICO_STORE_QUERY_MAX = 64;
  function picoStoreProviderTitle(store, providerId) {
    if (providerId === undefined || store.providers._tag !== "Ready")
      return;
    return store.providers.providers.find((provider) => provider.id === providerId)?.title;
  }
  function picoStoreSearchFor(store, providerId) {
    const search = store.search;
    return search._tag !== "Idle" && search.providerId === providerId ? search : { _tag: "Idle" };
  }

  // src/state/messages.ts
  var isHostButton = (message) => message._tag === "PressedBack" || message._tag === "PressedSystem" || message._tag === "PressedOptions" || message._tag === "PressedMenu";

  // src/state/update.ts
  function update(nav, message, korri) {
    const result = step3(settleLaunch(endNotices(nav, message), korri), message, korri);
    const settled = settleIdle(settleEditor(settleLaunch(settleSession(result.model, korri), korri), korri), korri);
    const identity = settleIdentity2(nav, settled, korri);
    const invites = settleInvites(identity.model, korri);
    return { model: invites.model, requests: [...result.requests, ...identity.requests, ...invites.requests] };
  }
  function step3(nav, message, korri) {
    if (runnerOpen(korri) && isHostButton(message)) {
      return message._tag === "PressedBack" ? ask(nav, { _tag: "RunAction", actionId: "runner:cancel" }) : stay(nav);
    }
    if (launchOpen(korri) && isHostButton(message)) {
      if (message._tag !== "PressedBack")
        return stay(nav);
      if (nav.launch !== undefined && nav.launch.question._tag !== "None")
        return stay(backInSettings(nav, nav.launch));
      const close = picoLaunchViewFromModel(korri)?.closeActionId;
      return close === undefined ? stay(nav) : ask(nav, { _tag: "RunAction", actionId: close });
    }
    if (isHostButton(message) || message._tag === "PressedScreen" || message._tag === "MovedFocus") {
      const waking = attractShowing(nav, korri);
      const activity = nav.idle.activity + 1;
      nav = { ...nav, idle: { _tag: "Awake", activity } };
      if (waking || !isHostButton(message))
        return stay(nav);
    }
    if (isHostButton(message) && korri.status._tag !== "Browsing") {
      return message._tag === "PressedBack" && (korri.status._tag === "Problem" || korri.status._tag === "Ended") ? ask(nav, { _tag: "DismissProblem" }) : stay(nav);
    }
    switch (message._tag) {
      case "PressedBack":
        return stay(back2(nav));
      case "PressedSystem":
        return stay(nav.settings !== undefined ? { ...nav, settings: undefined } : { ...nav, settings: SETTINGS, home: closeMenu(nav.home) });
      case "PressedOptions": {
        const top = topLayer(nav);
        if (top === "Find")
          return stay({ ...nav, find: false });
        if (top === "Home")
          return stay({ ...nav, find: true, home: closeMenu(nav.home) });
        return stay(nav);
      }
      case "PressedMenu":
        return stay(topLayer(nav) === "Home" ? cycleMode(nav) : nav);
      case "KorriPublished":
        return stay(nav);
      case "IdleElapsed":
        return stay(nav.idle._tag === "Awake" && canAttract(nav, korri) ? { ...nav, idle: { _tag: "Attracting", activity: nav.idle.activity } } : nav);
      case "ChoseRunnerAction":
        return ask(nav, { _tag: "RunAction", actionId: message.actionId });
      case "PressedLaunchAction":
        return picoLaunchViewFromModel(korri)?.actions.some((action) => action.id === message.actionId && action.enabled) === true ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "ToggledHomeMenu":
        return stay({ ...nav, home: { ...nav.home, menu: nav.home.menu === "Open" ? "Closed" : "Open" } });
      case "ChoseMenuFind":
        return stay({ ...nav, find: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } });
      case "ChoseMenuSettings":
        return stay({ ...nav, settings: SETTINGS, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } });
      case "ChoseMenuView":
        return stay(cycleMode(nav));
      case "ChoseMenuStore":
        return korri.store === undefined ? stay(nav) : ask({ ...nav, store: true, home: { ...closeMenu(nav.home), focusOnReturn: "MenuKey" } }, { _tag: "OpenStore" });
      case "AimedMenu":
        return stay(nav.home.aim === message.label ? nav : { ...nav, home: { ...nav.home, aim: message.label } });
      case "ReturnedFocus":
        return stay({ ...nav, home: { ...nav.home, focusOnReturn: "None" } });
      case "SelectedCart":
        return stay({ ...nav, home: { ...nav.home, selectedGameId: message.gameId } });
      case "OpenedGame": {
        const detail = { gameId: message.gameId, question: { _tag: "None" } };
        return stay(topLayer(nav) === "Home" ? { ...nav, detail, home: { ...closeMenu(nav.home), focusOnReturn: nav.home.mode === "shelf" ? "Cart" : "None" } } : { ...nav, detail });
      }
      case "PressedRetry":
        return ask(nav, picoScreenViewFromModel(korri)._tag === "Problem" ? { _tag: "Retry" } : { _tag: "Reload" });
      case "PressedDismiss":
        return ask(nav, { _tag: "DismissProblem" });
      case "PressedStatusAction": {
        const view = picoScreenViewFromModel(korri);
        return (view._tag === "Busy" || view._tag === "Confirm" || view._tag === "Paused") && (view.actions ?? []).some((action) => action.id === message.actionId && action.enabled) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      }
      case "PressedInviteAction":
        return korri.status._tag === "Browsing" && picoInviteActionPublished(korri, message.actionId) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "LostInviteFocus":
        return stay(topLayer(nav) === "Home" && nav.home.mode === "shelf" ? { ...nav, home: { ...nav.home, focusOnReturn: "Cart" } } : nav);
      case "TypedCharacter":
        return stay({ ...nav, search: { ...nav.search, query: nav.search.query + message.character } });
      case "PressedBackspace":
        return stay({ ...nav, search: { ...nav.search, query: nav.search.query.slice(0, -1) } });
      case "ClearedQuery":
        return stay({ ...nav, search: { ...nav.search, query: "" } });
      case "ChoseSection":
        return stay({ ...nav, search: { ...nav.search, section: message.section } });
      case "ChoseOrder":
        return stay({ ...nav, search: { ...nav.search, order: message.order } });
      case "ChoseStoreProvider": {
        const providers = korri.store?.providers;
        if (providers?._tag !== "Ready" || !providers.providers.some((provider) => provider.id === message.providerId))
          return stay(nav);
        const search = korri.store?.search;
        const searched = search !== undefined && search._tag !== "Idle" && search.providerId === message.providerId;
        return stay(withStore(nav, {
          view: "Results",
          providerId: message.providerId,
          typing: searched ? undefined : picoTextDraft("")
        }));
      }
      case "OpenedStoreSearch": {
        const search = korri.store?.search;
        const query = search !== undefined && search._tag !== "Idle" && search.providerId === nav.storePlace.providerId ? search.query : "";
        return nav.storePlace.view === "Results" ? stay(withStore(nav, { ...nav.storePlace, typing: picoTextDraft(query) })) : stay(nav);
      }
      case "ClosedStoreSearch":
        return stay(withStore(nav, { ...nav.storePlace, typing: undefined }));
      case "SubmittedStoreSearch": {
        const { providerId, typing } = nav.storePlace;
        if (providerId === undefined || typing === undefined)
          return stay(nav);
        return ask(withStore(nav, { ...nav.storePlace, typing: undefined }), { _tag: "SearchStore", providerId, query: typing.text.trim() });
      }
      case "OpenedStoreEntry": {
        const search = korri.store?.search;
        if (search?._tag !== "Ready" || !search.entries.some((entry) => entry.id === message.entryId))
          return stay(nav);
        return ask(withStore(nav, { ...nav.storePlace, view: "Entry", typing: undefined }), { _tag: "OpenStoreEntry", entryId: message.entryId });
      }
      case "PressedStoreAction":
        return storeActionPublished(korri, message.actionId) ? ask(nav, { _tag: "RunAction", actionId: message.actionId }) : stay(nav);
      case "PressedStorePlay": {
        const details = korri.store?.details;
        if (details?._tag !== "Ready" || details.get._tag !== "InLibrary" || details.get.gameId !== message.gameId)
          return stay(nav);
        const view = picoScreenViewFromModel(korri);
        const game = view._tag === "Shelf" ? view.games.find((candidate) => candidate.id === message.gameId) : undefined;
        if (game === undefined)
          return stay(nav);
        if (game.locations !== undefined && game.locations.length > 0) {
          return stay({ ...nav, detail: { gameId: game.id, question: { _tag: "ChoosingLocation", game } } });
        }
        return ask({ ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) }, { _tag: "LaunchGame", gameId: game.id });
      }
      case "PressedPlay": {
        const detail = nav.detail;
        if (detail === undefined)
          return stay(nav);
        const view = picoScreenViewFromModel(korri);
        const game = view._tag === "Shelf" ? view.games.find((candidate) => candidate.id === detail.gameId) : undefined;
        if (game === undefined)
          return stay(nav);
        if (game.locations !== undefined && game.locations.length > 0) {
          return stay({ ...nav, detail: { ...detail, question: { _tag: "ChoosingLocation", game } } });
        }
        return ask({ ...nav, session: picoSessionReturnOnPlay(nav.session, game.id, korri.catalog) }, { _tag: "LaunchGame", gameId: game.id });
      }
      case "ChoseLocation": {
        const detail = nav.detail;
        if (detail?.question._tag !== "ChoosingLocation")
          return stay(nav);
        const gameId = detail.question.game.id;
        return ask({
          ...nav,
          detail: { ...detail, question: { _tag: "None" } },
          session: picoSessionReturnOnPlay(nav.session, gameId, korri.catalog)
        }, { _tag: "LaunchGame", gameId, locationId: message.locationId });
      }
      case "PressedGameAction": {
        const detail = nav.detail;
        if (detail === undefined)
          return stay(nav);
        return message.action.destructive === true ? stay({ ...nav, detail: { ...detail, question: { _tag: "ConfirmingAction", action: message.action } } }) : ask(nav, { _tag: "RunGameAction", gameId: detail.gameId, actionId: message.action.id });
      }
      case "ConfirmedGameAction": {
        const detail = nav.detail;
        if (detail?.question._tag !== "ConfirmingAction")
          return stay(nav);
        return ask({ ...nav, detail: { ...detail, question: { _tag: "None" } } }, { _tag: "RunGameAction", gameId: detail.gameId, actionId: detail.question.action.id });
      }
      case "CancelledGameAction":
        return stay(nav.detail === undefined ? nav : { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } });
      case "ChoseSettingsGroup":
        if (nav.launch !== undefined)
          return stay({ ...nav, launch: { ...nav.launch, group: message.group } });
        return stay(nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, group: message.group } });
      case "PressedSettingAction": {
        if (message.actionId.startsWith("identity:")) {
          return stay(withQuestion(nav, { _tag: "Identity", actionId: message.actionId, form: IDENTITY_FORM, qr: undefined }));
        }
        if (message.actionId === PICO_WIFI_SCAN_ACTION) {
          const wifi = korri.network?.wifi;
          return wifi?._tag === "Available" && !wifi.scanning ? ask(nav, { _tag: "ScanWifi" }) : stay(nav);
        }
        if (message.actionId === PICO_WIFI_HIDDEN_ACTION)
          return stay(withQuestion(nav, { _tag: "Wifi", entry: HIDDEN_NAME_ENTRY }));
        const networkId = picoWifiNetworkId(message.actionId);
        if (networkId !== undefined)
          return joinFromSettings(nav, pickNetwork(korri, networkId));
        return ask(nav, { _tag: "RunAction", actionId: message.actionId });
      }
      case "AskedSettingConfirmation":
        return stay(withQuestion(nav, { _tag: "ConfirmingAction", actionId: message.actionId, confirmation: message.confirmation }));
      case "ConfirmedSettingAction": {
        const question = activeSettings(nav)?.question;
        if (question?._tag !== "ConfirmingAction")
          return stay(nav);
        return ask(withQuestion(nav, { _tag: "None" }), { _tag: "RunAction", actionId: question.actionId });
      }
      case "ChoseLaunchScope":
        return stay(nav.launch === undefined ? nav : { ...nav, launch: { ...nav.launch, scope: message.scope } });
      case "OpenedPicker":
        return pickRow(korri, message.settingId) === undefined ? stay(nav) : stay(withQuestion(nav, { _tag: "PickingChoice", settingId: message.settingId }));
      case "PickedChoice": {
        const question = activeSettings(nav)?.question;
        if (question?._tag !== "PickingChoice")
          return stay(nav);
        const row = pickRow(korri, question.settingId);
        if (row === undefined || !row.control.options.some((option) => option.value === message.value)) {
          return stay(withQuestion(nav, { _tag: "None" }));
        }
        return ask(withQuestion(nav, { _tag: "None" }), { _tag: "ChangeSetting", settingId: question.settingId, value: message.value });
      }
      case "CancelledPicker":
      case "CancelledSettingAction":
      case "ClosedEditor":
      case "ClosedIdentity":
        return stay(withQuestion(nav, { _tag: "None" }));
      case "ChangedSetting":
        if (message.settingId === PICO_FONT_SETTING) {
          return isPicoFont(message.value) ? ask(nav, { _tag: "ChooseFont", font: message.value }) : stay(nav);
        }
        return ask(nav, { _tag: "ChangeSetting", settingId: message.settingId, value: message.value });
      case "DismissedSettingsProblem":
        return ask(nav, { _tag: "DismissSettingsProblem" });
      case "OpenedEditor": {
        const row = textRow(korri, message.settingId);
        if (row === undefined)
          return stay(nav);
        const draft = picoTextDraft(row.control.sensitive ? "" : row.control.initial ?? row.value ?? "");
        return stay(withQuestion(nav, { _tag: "EditingText", settingId: row.id, draft, sawSaving: false }));
      }
      case "AskedClear": {
        const editor = openEditor(nav);
        return editor?._tag === "EditingText" && !saving(korri, editor) ? stay(withQuestion(nav, { ...editor, _tag: "ConfirmingClear" })) : stay(nav);
      }
      case "CancelledClear": {
        const editor = openEditor(nav);
        return editor?._tag === "ConfirmingClear" ? stay(withQuestion(nav, { ...editor, _tag: "EditingText" })) : stay(nav);
      }
      case "ConfirmedClear": {
        const editor = openEditor(nav);
        if (editor?._tag !== "ConfirmingClear")
          return stay(nav);
        return ask(withQuestion(nav, { ...editor, _tag: "EditingText" }), { _tag: "ChangeSetting", settingId: editor.settingId, value: "" });
      }
      case "TypedText":
        return stay(editDraft(nav, korri, (draft, maxLength) => picoDraftType(draft, message.character, maxLength)));
      case "PressedTextBackspace":
        return stay(editDraft(nav, korri, (draft) => picoDraftBackspace(draft)));
      case "ClearedText":
        return stay(editDraft(nav, korri, (draft) => ({ ...draft, text: "" })));
      case "ToggledCapitals":
        return stay(editDraft(nav, korri, (draft) => ({ ...draft, capitals: !draft.capitals })));
      case "ToggledSymbols":
        return stay(editDraft(nav, korri, (draft) => ({ ...draft, symbols: !draft.symbols })));
      case "ToggledShown": {
        const entry = wifiEntry(nav);
        return entry === undefined ? stay(nav) : stay(withQuestion(nav, { _tag: "Wifi", entry: toggleShown(entry) }));
      }
      case "SubmittedWifi": {
        const entry = wifiEntry(nav);
        return entry === undefined ? stay(nav) : joinFromSettings(nav, submitEntry(entry));
      }
      case "SavedText": {
        const editor = openEditor(nav);
        if (editor?._tag !== "EditingText" || saving(korri, editor))
          return stay(nav);
        if (editor.draft.text.trim() === "" && textRow(korri, editor.settingId)?.control.allowEmpty !== true)
          return stay(nav);
        const numeric = textRow(korri, editor.settingId)?.control.numeric;
        if (numeric !== undefined && !picoNumberFits(editor.draft.text, numeric))
          return stay(nav);
        return ask(nav, { _tag: "ChangeSetting", settingId: editor.settingId, value: editor.draft.text });
      }
      case "EditedIdentity":
        return stay(editIdentity(nav, (form) => ({ ...form, [message.field]: message.value })));
      case "ChoseIdentityDisposition":
        return stay(editIdentity(nav, (form) => ({ ...form, disposition: message.disposition })));
      case "CheckedIdentityConfirmation":
        return stay(editIdentity(nav, (form) => ({ ...form, confirmed: message.confirmed })));
      case "SubmittedIdentity": {
        const question = nav.settings?.question;
        if (question?._tag !== "Identity" || korri.identityManagement?.status._tag === "Working")
          return stay(nav);
        const request = identityRequest(question.actionId, question.form);
        return request === undefined ? stay(nav) : ask(nav, request);
      }
      case "RenderedQr": {
        const question = nav.settings?.question;
        return question?._tag === "Identity" && question.qr?.text === message.text ? stay(withQuestion(nav, { ...question, qr: { text: message.text, dataUrl: message.dataUrl } })) : stay(nav);
      }
      case "Waited":
        return stay(nav.invites.showing.some((notice) => notice.token === message.token) ? { ...nav, invites: { ...nav.invites, showing: nav.invites.showing.filter((notice) => notice.token !== message.token) } } : nav);
    }
  }
  function endNotices(nav, message) {
    const pressed = isHostButton(message) || message._tag === "PressedScreen";
    return pressed && nav.invites.showing.length > 0 ? { ...nav, invites: { ...nav.invites, showing: [] } } : nav;
  }
  function settleInvites(nav, korri) {
    const standing = korri.invites ?? [];
    const showing = nav.invites.showing.filter((notice) => standing.some((invite) => invite.id === notice.inviteId));
    const fresh = invitesNoticeable(korri) ? standing.filter((invite) => !nav.invites.noticed.includes(invite.id)) : [];
    if (fresh.length === 0 && showing.length === nav.invites.showing.length)
      return stay(nav);
    const added = fresh.map((invite, index) => ({ inviteId: invite.id, token: nav.invites.nextToken + index }));
    return {
      model: {
        ...nav,
        invites: {
          noticed: [...nav.invites.noticed, ...fresh.map((invite) => invite.id)],
          showing: [...[...added].reverse(), ...showing],
          nextToken: nav.invites.nextToken + added.length
        }
      },
      requests: added.map((notice) => ({ _tag: "Wait", ms: PICO_INVITE_NOTICE_MS, token: notice.token }))
    };
  }
  function back2(nav) {
    switch (topLayer(nav)) {
      case "Settings":
        return nav.settings === undefined ? nav : backInSettings(nav, nav.settings);
      case "Detail":
        if (nav.detail === undefined)
          return nav;
        return nav.detail.question._tag !== "None" ? { ...nav, detail: { ...nav.detail, question: { _tag: "None" } } } : { ...nav, detail: undefined };
      case "Store":
        return backInStore(nav, nav.storePlace);
      case "Find":
        return { ...nav, find: false };
      case "Home":
        return nav.home.menu === "Open" ? { ...nav, home: closeMenu(nav.home) } : nav;
    }
  }
  function backInStore(nav, place) {
    if (place.typing !== undefined)
      return withStore(nav, { ...place, typing: undefined });
    switch (place.view) {
      case "Entry":
        return withStore(nav, { ...place, view: "Results" });
      case "Results":
        return withStore(nav, { ...place, view: "Providers" });
      case "Providers":
        return { ...nav, store: false };
    }
  }
  function backInSettings(nav, settings) {
    const question = settings.question;
    switch (question._tag) {
      case "ConfirmingClear":
        return withQuestion(nav, { ...question, _tag: "EditingText" });
      case "ConfirmingAction":
      case "EditingText":
      case "PickingChoice":
      case "Identity":
      case "Wifi":
        return withQuestion(nav, { _tag: "None" });
      case "None":
        return { ...nav, settings: undefined };
    }
  }
  function settleSession(nav, korri) {
    const session = picoSessionReturnFromModel(nav.session, nav.detail?.gameId, korri);
    if (session === nav.session || session._tag === "Idle" && nav.session._tag === "Idle")
      return nav;
    if (session._tag !== "ReturnToLibrary")
      return { ...nav, session };
    return {
      ...nav,
      home: { ...nav.home, mode: "shelf", menu: "Closed", focusOnReturn: "None" },
      find: false,
      store: false,
      detail: undefined,
      settings: undefined,
      session: { _tag: "Idle" }
    };
  }
  function settleEditor(nav, korri) {
    const editor = openEditor(nav);
    if (editor === undefined)
      return nav;
    const state = textRow(korri, editor.settingId)?.state;
    if (state === "saving")
      return editor.sawSaving ? nav : withQuestion(nav, { ...editor, sawSaving: true });
    if (!editor.sawSaving)
      return nav;
    return withQuestion(nav, state === "idle" ? { _tag: "None" } : { ...editor, sawSaving: false });
  }
  function settleIdentity2(before, nav, korri) {
    const requests = [];
    if (identityAction(before) !== identityAction(nav))
      requests.push({ _tag: "DismissIdentityStatus" });
    const question = nav.settings?.question;
    if (question?._tag !== "Identity")
      return { model: nav, requests };
    const status = korri.identityManagement?.status;
    const text = status?._tag === "BackupReady" ? status.encryptedSecret : undefined;
    if (text === question.qr?.text)
      return { model: nav, requests };
    if (text === undefined)
      return { model: withQuestion(nav, { ...question, qr: undefined }), requests };
    requests.push({ _tag: "RenderQr", text });
    return { model: withQuestion(nav, { ...question, qr: { text, dataUrl: undefined } }), requests };
  }
  function identityRequest(actionId, form) {
    const exportKey = picoIdentityRetiredKey(actionId, "export");
    const deleteKey = picoIdentityRetiredKey(actionId, "delete");
    if (actionId === PICO_IDENTITY_BACKUP_ACTION || exportKey !== undefined) {
      return form.password.trim() === "" ? undefined : { _tag: "ExportIdentityBackup", password: form.password, retiredPublicKey: exportKey };
    }
    if (actionId === PICO_IDENTITY_SWITCH_LOCAL_ACTION) {
      return !form.confirmed || form.secret.trim() === "" || form.password.trim() === "" ? undefined : {
        _tag: "SwitchIdentityFromBackup",
        encryptedSecret: form.secret,
        password: form.password,
        disposition: form.disposition,
        trustLossConfirmed: form.confirmed
      };
    }
    if (actionId === PICO_IDENTITY_SWITCH_NIP46_ACTION) {
      return !form.confirmed || form.bunkerUri.trim() === "" ? undefined : {
        _tag: "SwitchIdentityToNip46",
        bunkerUri: form.bunkerUri,
        disposition: form.disposition,
        trustLossConfirmed: form.confirmed
      };
    }
    if (deleteKey !== undefined) {
      return form.confirmed ? { _tag: "DeleteRetiredIdentity", publicKey: deleteKey, backupConfirmed: form.confirmed } : undefined;
    }
    return;
  }
  var pickRow = (korri, settingId) => {
    const groups = [...picoLaunchViewFromModel(korri)?.settings.groups ?? [], ...picoSettingsViewFromModel(korri).groups];
    const row = groups.flatMap((group) => group.rows).find((candidate) => candidate.id === settingId);
    return row?.control.kind === "pick" ? row : undefined;
  };
  var textRow = (korri, settingId) => {
    const launch = picoLaunchViewFromModel(korri)?.settings;
    return (launch === undefined ? undefined : picoTextSettingFrom(launch, settingId)?.row) ?? picoTextSettingFrom(picoSettingsViewFromModel(korri), settingId)?.row;
  };
  function settleLaunch(nav, korri) {
    const open = launchOpen(korri);
    if (open && nav.launch === undefined)
      return { ...nav, launch: SETTINGS };
    if (!open && nav.launch !== undefined)
      return { ...nav, launch: undefined };
    return nav;
  }
  var activeSettings = (nav) => nav.launch ?? nav.settings;
  var openEditor = (nav) => {
    const question = activeSettings(nav)?.question;
    return question?._tag === "EditingText" || question?._tag === "ConfirmingClear" ? question : undefined;
  };
  var saving = (korri, editor) => textRow(korri, editor.settingId)?.state === "saving";
  function editDraft(nav, korri, change) {
    const entry = wifiEntry(nav);
    if (entry !== undefined) {
      return withQuestion(nav, { _tag: "Wifi", entry: editEntry(entry, (draft) => change(draft, entryMaxLength(entry))) });
    }
    const typing = nav.storePlace.typing;
    if (topLayer(nav) === "Store" && typing !== undefined) {
      return withStore(nav, { ...nav.storePlace, typing: change(typing, PICO_STORE_QUERY_MAX) });
    }
    const editor = openEditor(nav);
    if (editor?._tag !== "EditingText")
      return nav;
    const row = textRow(korri, editor.settingId);
    if (row === undefined || row.state === "saving")
      return nav;
    return withQuestion(nav, { ...editor, draft: change(editor.draft, row.control.maxLength) });
  }
  var wifiEntry = (nav) => {
    const question = nav.settings?.question;
    return question?._tag === "Wifi" ? question.entry : undefined;
  };
  function joinFromSettings(nav, next) {
    switch (next._tag) {
      case "Edit":
        return stay(withQuestion(nav, { _tag: "Wifi", entry: next.entry }));
      case "Send":
        return ask(withQuestion(nav, { _tag: "None" }), next.request);
      case "Nothing":
        return stay(nav);
    }
  }
  function editIdentity(nav, change) {
    const question = nav.settings?.question;
    return question?._tag === "Identity" ? withQuestion(nav, { ...question, form: change(question.form) }) : nav;
  }
  function settleIdle(nav, korri) {
    return nav.idle._tag === "Attracting" && !canAttract(nav, korri) ? { ...nav, idle: { _tag: "Awake", activity: nav.idle.activity } } : nav;
  }
  var withStore = (nav, place) => ({ ...nav, storePlace: place });
  function storeActionPublished(korri, actionId) {
    const store = korri.store;
    if (store === undefined)
      return false;
    const lists = [
      store.providers._tag === "Problem" ? store.providers.actions : [],
      store.search._tag === "Problem" ? store.search.actions : [],
      store.details._tag === "Problem" ? store.details.actions : [],
      store.details._tag === "Ready" && "actions" in store.details.get ? store.details.get.actions : []
    ];
    return lists.some((actions) => actions.some((action) => action.id === actionId && action.enabled));
  }
  var closeMenu = (home) => home.menu === "Closed" ? home : { ...home, menu: "Closed" };
  var withQuestion = (nav, question) => nav.launch !== undefined ? { ...nav, launch: { ...nav.launch, question } } : nav.settings === undefined ? nav : { ...nav, settings: { ...nav.settings, question } };
  var cycleMode = (nav) => ({
    ...nav,
    home: { ...nav.home, mode: nav.home.mode === "shelf" ? "grid" : nav.home.mode === "grid" ? "hero" : "shelf" }
  });

  // src/pico-detail-view.ts
  function picoDetailViewFromGame(game) {
    const download = picoDownloadLabel(game);
    return {
      id: game.id,
      title: game.title,
      ...game.subtitle === undefined ? {} : { subtitle: game.subtitle },
      ...game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl },
      primaryLabel: game.resumable === true ? "CONTINUE" : "PLAY",
      ...game.activeProfile === undefined ? {} : { profile: game.activeProfile },
      stats: picoStatsFor(game),
      ...download === undefined ? {} : { download },
      ...game.download?.progress === undefined ? {} : { downloadProgress: game.download.progress },
      ...game.launchLocations === undefined || game.launchLocations.length === 0 ? {} : { locations: game.launchLocations }
    };
  }
  function picoStatsFor(game) {
    const stats = [];
    if (game.playCount !== undefined) {
      stats.push({
        figure: String(game.playCount),
        caption: game.playCount === 1 ? "PLAY" : "PLAYS"
      });
    }
    if (game.totalPlaytimeSeconds !== undefined) {
      stats.push({ figure: playtimeLabel(game.totalPlaytimeSeconds), caption: "PLAYED" });
    }
    return stats;
  }
  function picoDownloadLabel(game) {
    const download = game.download;
    if (download === undefined)
      return;
    if (download.progress !== undefined) {
      const percent = picoPercent(download.progress);
      return percent === undefined ? "DOWNLOADING" : `DOWNLOADING ${percent}%`;
    }
    if (download.queued === true)
      return "QUEUED";
    return download.bytes === undefined ? "DOWNLOAD NEEDED" : `${picoBytesLabel(download.bytes)} DOWNLOAD`;
  }
  function picoPercent(progress) {
    if (progress.total === undefined || progress.total <= 0)
      return;
    return Math.max(0, Math.min(100, Math.floor(progress.done / progress.total * 100)));
  }
  function picoBytesLabel(bytes) {
    const scale = (value, unit) => {
      const text = value < 10 ? value.toFixed(1) : String(Math.round(value));
      return `${text.endsWith(".0") ? text.slice(0, -2) : text} ${unit}`;
    };
    if (bytes >= 1e9)
      return scale(bytes / 1e9, "GB");
    if (bytes >= 1e6)
      return scale(bytes / 1e6, "MB");
    if (bytes >= 1000)
      return scale(bytes / 1000, "KB");
    return `${Math.round(bytes)} B`;
  }
  function playtimeLabel(seconds) {
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const rest = minutes % 60;
    if (hours === 0)
      return `${rest}M`;
    return rest === 0 ? `${hours}H` : `${hours}H ${rest}M`;
  }

  // src/pico-initials.ts
  function picoInitials(title) {
    const words = title.trim().split(/\s+/).filter(Boolean);
    if (words.length === 0)
      return "??";
    if (words.length === 1)
      return (words[0] ?? "").slice(0, 2).toUpperCase();
    return words.slice(0, 2).map((word) => word.charAt(0)).join("").toUpperCase();
  }

  // src/pico-palette.ts
  var of = (name, hex) => ({
    name,
    hex,
    rgb: [
      Number.parseInt(hex.slice(1, 3), 16),
      Number.parseInt(hex.slice(3, 5), 16),
      Number.parseInt(hex.slice(5, 7), 16)
    ]
  });
  var PICO_PALETTE = [
    of("black", "#000000"),
    of("navy", "#1d2b53"),
    of("maroon", "#7e2553"),
    of("green", "#008751"),
    of("brown", "#ab5236"),
    of("slate", "#5f574f"),
    of("silver", "#c2c3c7"),
    of("white", "#fff1e8"),
    of("red", "#ff004d"),
    of("orange", "#ffa300"),
    of("yellow", "#ffec27"),
    of("lime", "#00e436"),
    of("blue", "#29adff"),
    of("lilac", "#83769c"),
    of("pink", "#ff77a8"),
    of("peach", "#ffccaa")
  ];
  var PICO_SHELL_COLORS = [
    12,
    14,
    9,
    11,
    15,
    6,
    8,
    10,
    3,
    13
  ].map((index) => PICO_PALETTE[index]);
  function picoLuminance(color) {
    const [r, g, b] = color.rgb;
    return 0.299 * r + 0.587 * g + 0.114 * b;
  }

  // src/pico-label.ts
  function hash(input) {
    let h = 2166136261;
    for (let index = 0;index < input.length; index += 1) {
      h ^= input.charCodeAt(index);
      h = Math.imul(h, 16777619);
    }
    return h >>> 0;
  }
  var INK_FLIP = 140;
  var SYSTEMS = {
    "PICO-8": { system: "pico8", shell: 4 },
    GB: { system: "gb", shell: 5 },
    GBC: { system: "gbc", shell: 7 },
    GBA: { system: "gba", shell: 0 },
    NES: { system: "nes", shell: 6 },
    FC: { system: "nes", shell: 6 },
    SNES: { system: "snes", shell: 9 },
    SFC: { system: "snes", shell: 9 },
    MD: { system: "md", shell: 8 },
    GENESIS: { system: "md", shell: 8 }
  };
  function picoCartLookFor(gameId, subtitle) {
    const label = picoLabelFor(gameId);
    const word = subtitle?.split("·")[0]?.trim().toUpperCase() ?? "";
    const known = SYSTEMS[word];
    const shell = known?.shell ?? label.shell;
    const sticker = label.sticker !== shell ? label.sticker : label.shell;
    const color = PICO_SHELL_COLORS[sticker];
    return {
      system: known?.system ?? "other",
      shell,
      sticker,
      ink: color !== undefined && picoLuminance(color) < INK_FLIP ? "light" : "dark"
    };
  }
  function picoLabelFor(gameId) {
    const h = hash(gameId);
    const shell = h % PICO_SHELL_COLORS.length;
    const offset = 1 + (h >>> 8) % (PICO_SHELL_COLORS.length - 1);
    const sticker = (shell + offset) % PICO_SHELL_COLORS.length;
    const stickerColor = PICO_SHELL_COLORS[sticker];
    return {
      shell,
      sticker,
      ink: stickerColor !== undefined && picoLuminance(stickerColor) < INK_FLIP ? "light" : "dark"
    };
  }

  // src/native/kit.ts
  var button = (label, options = {}) => ({
    label,
    detail: "",
    focused: false,
    disabled: false,
    warn: false,
    lit: false,
    ...options
  });
  var NO_CARD = {
    shown: false,
    tone: "ask",
    kicker: "",
    title: "",
    message: "",
    lines: [],
    actions: [],
    inline: false
  };
  var card = (options) => ({ ...NO_CARD, shown: true, ...options });
  var chip = (text, kind, caption = "") => ({ text, caption, kind });
  var NO_BAR = { shown: false, cells: 0, walking: false, label: "" };
  function bar(progress, label = "") {
    if (progress === undefined)
      return { ...NO_BAR, shown: true, walking: true, label };
    if (progress.total === undefined || progress.total <= 0)
      return { shown: true, cells: 0, walking: true, label };
    const cells = Math.max(0, Math.min(16, Math.floor(progress.done / progress.total * 16)));
    return { shown: true, cells, walking: false, label };
  }
  var art = (url) => url === undefined || url === "" ? undefined : { $image: url, pico8: "vivid" };
  function cartView(game, options = {}) {
    const look = picoCartLookFor(game.id, game.subtitle);
    const cover = art(game.artUrl);
    const progress = game.resumable === true ? "resume" : picoStatsFor(game).length === 0 && options.band !== false ? "new" : "played";
    return {
      id: game.id,
      title: game.title,
      ...cover === undefined ? {} : { art: cover },
      hasArt: cover !== undefined,
      artRatio: artRatio(game.artUrl),
      initials: picoInitials(game.title),
      system: look.system,
      shell: look.shell,
      sticker: look.sticker,
      stickerInkLight: look.ink === "light",
      progress,
      hero: options.hero === true,
      focused: options.focused === true
    };
  }
  var BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  function pngSize(url) {
    const comma = url.indexOf(",");
    if (!url.startsWith("data:image/png;base64,") || comma === -1)
      return;
    const bytes = [];
    let bits = 0;
    let value = 0;
    for (const char of url.slice(comma + 1, comma + 1 + 32)) {
      const digit = BASE64.indexOf(char);
      if (digit === -1)
        break;
      value = (value << 6 | digit) & 16777215;
      bits += 6;
      if (bits >= 8) {
        bits -= 8;
        bytes.push(value >> bits & 255);
      }
    }
    if (bytes.length < 24)
      return;
    const word = (at2) => ((bytes[at2] ?? 0) << 24 | (bytes[at2 + 1] ?? 0) << 16 | (bytes[at2 + 2] ?? 0) << 8 | (bytes[at2 + 3] ?? 0)) >>> 0;
    const width = word(16);
    const height = word(20);
    return width > 0 && height > 0 ? { width, height } : undefined;
  }
  function artRatio(url) {
    if (url === undefined || url === "")
      return 1;
    const size = pngSize(url);
    if (size === undefined)
      return 0;
    const shape = size.width / size.height;
    return Math.max(1, Math.round(64 * Math.sqrt(shape))) / Math.max(1, Math.round(64 / Math.sqrt(shape)));
  }
  function narrowestArt(carts) {
    return carts.reduce((narrowest, cart) => Math.min(narrowest, cart.hasArt ? cart.artRatio : 1), carts.length === 0 ? 1 : Number.POSITIVE_INFINITY);
  }
  var A = (label) => ({ key: "a", label });
  var B = (label) => ({ key: "b", label });

  // src/native/frame.ts
  function frame(options) {
    return {
      place: "korri",
      sep: false,
      clock: "",
      readout: "",
      hints: [],
      hasLead: false,
      leadKind: "menu",
      lead: button(""),
      track: [],
      ...options
    };
  }

  // src/native/keyboard.ts
  var SEARCH_ROWS = ["ABCDEFGHIJ", "KLMNOPQRST", "UVWXYZ0123", "456789"];
  var LETTER_ROWS = ["abcdefghij", "klmnopqrst", "uvwxyz0123", "456789"];
  var SYMBOL_ROWS = [`!"#$%&'()*`, "+,-./:;<=>", "?@[\\]^_`{|", "}~"];
  function layout(charset, draft) {
    const rows = charset === "search" ? SEARCH_ROWS : draft.symbols ? SYMBOL_ROWS : draft.capitals ? LETTER_ROWS.map((row) => row.toUpperCase()) : LETTER_ROWS;
    const keys = [];
    rows.forEach((characters, row) => {
      [...characters].forEach((character, col) => {
        keys.push({ id: `key:${row}:${col}`, cap: character, row, col, cols: 1, press: { _tag: "Type", character }, lit: false });
      });
    });
    keys.push({ id: "key:space", cap: "SPACE", row: 3, col: 6, cols: 2, press: { _tag: "Type", character: " " }, lit: false });
    keys.push({ id: "key:del", cap: "DEL", row: 3, col: 8, cols: 1, press: { _tag: "Backspace" }, lit: false });
    keys.push({ id: "key:clear", cap: "CLEAR", row: 3, col: 9, cols: 1, press: { _tag: "Clear" }, lit: false });
    if (charset === "text") {
      keys.push({ id: "key:caps", cap: "↑ CAPS", row: 4, col: 0, cols: 5, press: { _tag: "Capitals" }, lit: draft.capitals });
      keys.push({ id: "key:symbols", cap: draft.symbols ? "abc" : "#+=", row: 4, col: 5, cols: 5, press: { _tag: "Symbols" }, lit: draft.symbols });
    }
    return keys;
  }
  function keyboard(charset, draft, focused, top, act) {
    const placed = layout(charset, draft);
    const box = (key) => ({ x: key.col * 10, y: top + key.row * 10, w: key.cols * 10, h: 8 });
    return {
      keys: placed.map((key) => ({ cap: key.cap, focused: key.id === focused, lit: key.lit, row: key.row, col: key.col, cols: key.cols })),
      rows: charset === "text" ? 5 : 4,
      targets: placed.map((key) => ({ id: key.id, box: box(key), act: act(key.press) }))
    };
  }

  // src/native/types.ts
  var nav = (message) => ({ to: "nav", message });
  var setup = (message) => ({ to: "setup", message });
  var local = (change) => ({ to: "local", change });

  // src/native/catalog.ts
  var MENU_LABEL = "MENU";
  var MODE_LABELS = { shelf: "LIBRARY", grid: "LIBRARY · GRID", hero: "LIBRARY · HERO" };
  var VIEW_WORDS = { shelf: "SHELF", grid: "GRID", hero: "HERO" };
  var EMPTY_CART = cartView({ id: "", title: "" });
  var NO_STAGE = { shown: false, title: "", subtitle: "", kicker: "", chips: [], index: 0, total: 0, cart: EMPTY_CART };
  function factChips(game) {
    const stats = picoStatsFor(game);
    const download = picoDownloadLabel(game);
    return [
      ...game.resumable === true ? [chip("RESUME", "ok")] : [],
      ...download === undefined ? [] : [chip(download, "info")],
      ...stats.length === 0 ? [chip("NEVER PLAYED", "empty")] : stats.map((stat) => chip(stat.figure, "stat", stat.caption))
    ];
  }
  var focusId = (ctx) => ctx.s.focus;
  function cardTargets(shown, ids, acts, focused, lines = 0) {
    const targets = [];
    for (let i = 0;i < lines; i += 1)
      targets.push({ id: `line:${i}`, box: { x: 0, y: i * 10, w: 100, h: 8 } });
    shown.actions.forEach((_, i) => {
      const id = ids[i] ?? `answer:${i}`;
      const act = acts[i];
      if (shown.actions[i]?.disabled === true)
        return;
      targets.push({
        id,
        box: shown.inline ? { x: i * 50, y: lines * 10 + 10, w: 48, h: 8 } : { x: 0, y: (lines + i) * 10 + 10, w: 100, h: 8 },
        ...act === undefined ? {} : { act }
      });
    });
    return {
      card: {
        ...shown,
        lines: shown.lines.map((line, i) => ({ ...line, focused: focused === `line:${i}` })),
        actions: shown.actions.map((action, i) => ({ ...action, focused: focused === (ids[i] ?? `answer:${i}`) }))
      },
      targets
    };
  }
  function homeHints(view, menuOffered, menuOpen, aim) {
    if (menuOpen)
      return [...aim === undefined || aim === MENU_LABEL ? [] : [A(aim)], B("CLOSE")];
    if (menuOffered && aim === MENU_LABEL)
      return [A(MENU_LABEL), B("BACK")];
    if (view._tag === "Shelf")
      return [A("PLAY"), B("BACK")];
    if (view._tag === "Busy" && (view.actions?.length ?? 0) === 0 || view._tag === "Paused" && view.actions.length === 0)
      return [];
    return [B("BACK")];
  }
  function shelfReadout(games, total) {
    const resumable = games.filter((game) => game.resumable === true).length;
    const carts = `${total} ${total === 1 ? "CART" : "CARTS"}`;
    return resumable === 0 ? carts : `${carts} · ${resumable} RESUMABLE`;
  }
  function chosenIndex(games, selected) {
    const found = games.findIndex((game) => game.id === selected);
    return found === -1 ? 0 : found;
  }
  var statusActions = (actions, focused) => (actions ?? []).map((action) => button(action.label, {
    detail: action.description ?? "",
    disabled: !action.enabled,
    focused: focused === `status:${action.id}`
  }));
  function gridColumns(width) {
    return Math.max(1, Math.floor((width - 16 + 4) / 41));
  }
  function homeScreen(ctx, shown) {
    const { s, korri, model } = ctx;
    const view = shown.view;
    const home = shown.home;
    const focused = focusId(ctx);
    const library = view._tag === "Shelf" || view._tag === "Loading" || view._tag === "Empty" || view._tag === "Failed";
    const menuOpen = library && home.menu === "Open";
    const targets = [];
    const games = view._tag === "Shelf" ? view.games : [];
    const catalog = korri.catalog;
    const firstIndex = catalog._tag === "Ready" ? catalog.firstIndex : 0;
    const total = catalog._tag === "Ready" ? catalog.total : games.length;
    const chosen = chosenIndex(games, home.selectedGameId);
    const chosenGame = games[chosen];
    const chosenId = chosenGame === undefined ? undefined : `cart:${chosenGame.id}`;
    const entries = [
      { label: "FIND", detail: "", act: [nav({ _tag: "ChoseMenuFind" })] },
      ...model.store === undefined ? [] : [{ label: "GET GAMES", detail: "", act: [nav({ _tag: "ChoseMenuStore" })] }],
      { label: "SETTINGS", detail: "", act: [nav({ _tag: "ChoseMenuSettings" })] },
      { label: "VIEW", detail: `◀ ${VIEW_WORDS[home.mode]} ▶`, act: [nav({ _tag: "ChoseMenuView" })] }
    ];
    const aimAt = (label) => [nav({ _tag: "AimedMenu", label })];
    const menuKey = {
      id: "menu",
      box: { x: 0, y: 1000, w: 10, h: 8 },
      act: [nav({ _tag: "ToggledHomeMenu" })],
      enter: aimAt(MENU_LABEL),
      ...menuOpen ? {} : chosenId === undefined ? {} : { next: { up: chosenId } }
    };
    let body = "none";
    let stage = NO_STAGE;
    let carts = [];
    let sections = [];
    let hero = { title: "", subtitle: "", reason: "", chips: [], open: button("OPEN"), resume: [] };
    let shownCard = NO_CARD;
    let launch = { kicker: "", detail: "", bar: NO_BAR, hasCart: false, cart: EMPTY_CART, phase: "starting", actions: [] };
    let invites = [];
    if (menuOpen) {
      entries.forEach((entry, i) => targets.push({
        id: `menu:${entry.label}`,
        box: { x: 0, y: 900 + i * 10, w: 30, h: 8 },
        act: entry.act,
        enter: aimAt(entry.label)
      }));
      targets.push(menuKey);
    }
    if (library) {
      const standing = picoInvitesFrom(model);
      invites = standing.map((invite, row) => {
        const actions = invite.actions.map((action, column) => {
          const id = `invite:${action.id}`;
          if (!menuOpen && action.enabled) {
            targets.push({
              id,
              box: { x: 50 + column * 20, y: row * 10, w: 18, h: 8 },
              act: [nav({ _tag: "PressedInviteAction", actionId: action.id })],
              enter: aimAt(undefined)
            });
          }
          return button(action.label, { disabled: !action.enabled, focused: focused === id });
        });
        const cover = art(invite.artUrl);
        return {
          title: invite.gameTitle,
          who: `${picoInvitePlaying(invite.personName)} ${picoInviteWhere(invite.deviceLabel)}`,
          ...cover === undefined ? {} : { art: { $image: cover.$image, pico8: "vivid" } },
          hasArt: cover !== undefined,
          actions
        };
      });
    }
    if (view._tag === "Shelf" && home.mode === "shelf") {
      body = "shelf";
      carts = games.map((game, i) => {
        const id = `cart:${game.id}`;
        if (!menuOpen) {
          targets.push({
            id,
            box: { x: (firstIndex + i) * 10, y: 500, w: 9, h: 20 },
            act: [nav({ _tag: "OpenedGame", gameId: game.id })],
            enter: [nav({ _tag: "SelectedCart", gameId: game.id }), ...home.aim === undefined ? [] : aimAt(undefined)],
            next: { down: "menu" }
          });
        }
        return cartView(game, { hero: i === chosen, focused: focused === id });
      });
      if (chosenGame !== undefined) {
        stage = {
          shown: true,
          title: chosenGame.title,
          subtitle: chosenGame.subtitle ?? "",
          kicker: (chosenGame.section ?? "").toUpperCase(),
          chips: factChips(chosenGame),
          index: firstIndex + chosen + 1,
          total,
          cart: cartView(chosenGame, { hero: true })
        };
      }
    } else if (view._tag === "Shelf" && home.mode === "grid") {
      body = "grid";
      const columns = gridColumns(ctx.plan.width);
      let rowStart = 0;
      sections = picoCollectionsFrom(games).map((collection, index) => {
        const section = {
          title: collection.title,
          rowStart,
          carts: collection.games.map((game, i) => {
            const id = `cart:${game.id}`;
            if (!menuOpen) {
              const row = rowStart + Math.floor(i / columns);
              targets.push({
                id,
                box: { x: i % columns * 10, y: row * 30 + index * 10, w: 9, h: 20 },
                act: [nav({ _tag: "OpenedGame", gameId: game.id })],
                enter: [nav({ _tag: "SelectedCart", gameId: game.id }), ...home.aim === undefined ? [] : aimAt(undefined)]
              });
            }
            return cartView(game, { focused: focused === id, hero: focused === id, band: false });
          })
        };
        rowStart += Math.ceil(collection.games.length / columns);
        return section;
      });
    } else if (view._tag === "Shelf" && home.mode === "hero") {
      body = "hero";
      const pick = picoHeroPick(games);
      const resumable = games.filter((game) => game.resumable === true);
      if (pick !== undefined && !menuOpen) {
        targets.push({ id: "hero:open", box: { x: 0, y: 100, w: 30, h: 10 }, act: [nav({ _tag: "OpenedGame", gameId: pick.game.id })], enter: aimAt(undefined) });
      }
      resumable.forEach((game, i) => {
        if (!menuOpen)
          targets.push({ id: `resume:${game.id}`, box: { x: i * 10, y: 300, w: 9, h: 20 }, act: [nav({ _tag: "OpenedGame", gameId: game.id })], enter: aimAt(undefined) });
      });
      hero = pick === undefined ? hero : {
        title: pick.game.title,
        subtitle: pick.game.subtitle ?? "",
        reason: pick.reason ?? "",
        chips: factChips(pick.game),
        open: button("OPEN", { focused: focused === "hero:open" }),
        resume: resumable.map((game) => ({
          cart: cartView(game, { focused: focused === `resume:${game.id}`, hero: focused === `resume:${game.id}`, band: false }),
          title: game.title,
          subtitle: game.subtitle ?? ""
        }))
      };
    } else if (view._tag === "Loading" || view._tag === "Empty" || view._tag === "Failed") {
      body = "card";
      const notice = view._tag === "Loading" ? card({ tone: "tell", kicker: "READING CARTS", message: "Korri is looking through your library." }) : view._tag === "Empty" ? card({ tone: "tell", kicker: "NO CARTS", message: "Nothing to play yet. Add games to your library and they appear here." }) : card({ tone: "warn", kicker: "SHELF JAMMED", message: view.message, actions: [button("TRY AGAIN")] });
      const placed = cardTargets(notice, ["retry"], [[nav({ _tag: "PressedRetry" })]], focused);
      if (!menuOpen)
        targets.push(...placed.targets.map((target) => ({ ...target, box: { ...target.box, y: target.box.y + 500 } })));
      shownCard = placed.card;
    } else if (view._tag === "Confirm") {
      body = "card";
      const ids = view.actions.map((action) => `status:${action.id}`);
      const placed = cardTargets(card({ tone: "ask", kicker: view.kicker, title: view.cart?.title ?? "", message: view.detail ?? "", actions: view.actions.map((action) => button(action.label, { disabled: !action.enabled })) }), ids, view.actions.map((action) => [nav({ _tag: "PressedStatusAction", actionId: action.id })]), focused);
      targets.push(...placed.targets);
      shownCard = placed.card;
    } else if (view._tag === "Problem") {
      body = "card";
      const answers = view.canRetry ? [button("RETRY"), button("CANCEL")] : [button("OK")];
      const placed = cardTargets(card({ tone: "warn", kicker: view.kicker, title: view.gameTitle ?? "", message: view.reason, actions: answers }), view.canRetry ? ["retry", "dismiss"] : ["dismiss"], view.canRetry ? [[nav({ _tag: "PressedRetry" })], [nav({ _tag: "PressedDismiss" })]] : [[nav({ _tag: "PressedDismiss" })]], focused);
      targets.push(...placed.targets);
      shownCard = placed.card;
    } else if (view._tag === "Ended") {
      body = "card";
      const placed = cardTargets(card({ tone: "tell", kicker: view.kicker, message: view.reason, actions: [button("OK")] }), ["dismiss"], [[nav({ _tag: "PressedDismiss" })]], focused);
      targets.push(...placed.targets);
      shownCard = placed.card;
    } else if (view._tag === "Busy" || view._tag === "Running" || view._tag === "Paused") {
      body = "stage";
      const actions = view._tag === "Running" ? [] : view.actions ?? [];
      actions.forEach((action, i) => {
        if (action.enabled)
          targets.push({ id: `status:${action.id}`, box: { x: i * 20, y: 0, w: 18, h: 8 }, act: [nav({ _tag: "PressedStatusAction", actionId: action.id })] });
      });
      const progress = view._tag === "Busy" ? view.progress : undefined;
      launch = {
        kicker: view.kicker,
        detail: view._tag === "Running" ? view.gameTitle ?? "" : view.detail ?? "",
        bar: view._tag === "Busy" ? progress === undefined ? { ...NO_BAR, shown: true, cells: 16 } : bar(progress) : NO_BAR,
        hasCart: view.cart !== undefined,
        cart: view.cart === undefined ? EMPTY_CART : cartView({ id: view.cart.id, title: view.cart.title, ...view.cart.artUrl === undefined ? {} : { artUrl: view.cart.artUrl } }, { band: false }),
        phase: view._tag === "Busy" ? "starting" : view._tag === "Running" ? "running" : "paused",
        actions: statusActions(actions, focused)
      };
    }
    const menuOffered = library;
    if (menuOffered && !menuOpen)
      targets.push(menuKey);
    const fallback = home.focusOnReturn === "MenuKey" ? "menu" : menuOpen ? "menu:FIND" : view._tag === "Confirm" && view.declineActionId !== undefined ? `status:${view.declineActionId}` : chosenId ?? targets[0]?.id;
    return {
      key: `home:${home.mode}:${view._tag}:${menuOpen ? "menu" : ""}`,
      fallback,
      targets,
      view: {
        page: "home",
        frame: frame({
          label: MODE_LABELS[home.mode],
          clock: korri.clockLabel ?? "",
          readout: view._tag === "Shelf" ? shelfReadout(games, total) : "",
          hints: homeHints(view, menuOffered, menuOpen, home.aim),
          hasLead: menuOffered,
          lead: button(MENU_LABEL, { focused: focused === "menu", lit: menuOpen })
        }),
        card: shownCard,
        home: {
          body,
          mode: home.mode,
          carts,
          selectedIndex: chosen,
          firstIndex,
          total,
          cursorOnShelf: focused !== undefined && focused.startsWith("cart:"),
          narrowestArt: narrowestArt(carts),
          stage,
          invites,
          invitesLabel: `${invites.length} ${invites.length === 1 ? "GAME" : "GAMES"}`,
          menuOpen,
          menu: entries.map((entry) => button(entry.label, { detail: entry.detail, focused: focused === `menu:${entry.label}` })),
          sections,
          columns: gridColumns(ctx.plan.width),
          focusRow: focusedRow(targets, focused),
          hero,
          launch
        }
      }
    };
  }
  function focusedRow(targets, focused) {
    const target = targets.find((candidate) => candidate.id === focused);
    return target === undefined || target.box.y >= 900 ? 0 : Math.floor(target.box.y / 30);
  }
  function findScreen(ctx, shown) {
    const { s, korri, model } = ctx;
    const search = shown.search;
    const focused = focusId(ctx);
    const targets = [];
    const sections = [PICO_ALL_SECTIONS, ...s.sections];
    const sectionButtons = sections.map((section, i) => {
      const id = `section:${section}`;
      targets.push({ id, box: { x: i * 10, y: 10, w: 9, h: 8 }, act: [nav({ _tag: "ChoseSection", section })] });
      return button(section.toUpperCase(), { lit: section === search.section, focused: focused === id });
    });
    const orderButtons = PICO_ORDERS.map((order, i) => {
      const id = `order:${order}`;
      targets.push({ id, box: { x: i * 10, y: 20, w: 9, h: 8 }, act: [nav({ _tag: "ChoseOrder", order })] });
      return button(PICO_ORDER_LABELS[order], { lit: order === search.order, focused: focused === id });
    });
    const keys = keyboard("search", { capitals: false, symbols: false }, focused, 30, (press) => press._tag === "Type" ? [nav({ _tag: "TypedCharacter", character: press.character })] : press._tag === "Backspace" ? [nav({ _tag: "PressedBackspace" })] : [nav({ _tag: "ClearedQuery" })]);
    targets.push(...keys.targets);
    const catalog = korri.catalog;
    const answered = catalog._tag === "Ready" && sameQuery(catalog.query, findQuery(search));
    const games = answered && model.catalog._tag === "Ready" ? model.catalog.games : [];
    const firstIndex = catalog._tag === "Ready" ? catalog.firstIndex : 0;
    const results = games.map((game) => {
      const id = `result:${game.id}`;
      targets.push({ id, box: { x: (firstIndex + games.indexOf(game)) * 10, y: 100, w: 9, h: 20 }, act: [nav({ _tag: "OpenedGame", gameId: game.id })] });
      const shelf = {
        id: game.id,
        title: game.title,
        ...game.subtitle === undefined ? {} : { subtitle: game.subtitle },
        ...game.coverArtUrl === undefined ? {} : { artUrl: game.coverArtUrl },
        ...game.resumable === undefined ? {} : { resumable: game.resumable },
        ...game.playCount === undefined ? {} : { playCount: game.playCount },
        ...game.totalPlaytimeSeconds === undefined ? {} : { totalPlaytimeSeconds: game.totalPlaytimeSeconds }
      };
      return { cart: cartView(shelf, { focused: focused === id, hero: focused === id, band: false }), title: game.title };
    });
    const focusedResult = results.findIndex((result) => focused === `result:${result.cart.id}`);
    return {
      key: "find",
      fallback: "key:0:0",
      targets,
      view: {
        page: "find",
        frame: frame({ label: "FIND", clock: korri.clockLabel ?? "", hints: [A("OPEN"), B("BACK")] }),
        card: NO_CARD,
        find: {
          query: search.query,
          sections: sectionButtons,
          orders: orderButtons,
          keys: keys.keys,
          keyRows: keys.rows,
          results,
          focusedResult: Math.max(0, focusedResult),
          empty: answered && results.length === 0,
          count: catalog._tag === "Ready" && answered ? `${catalog.total} ${catalog.total === 1 ? "GAME" : "GAMES"}` : ""
        }
      }
    };
  }
  function findQuery(search) {
    return {
      text: search.query.trim(),
      ...search.section === PICO_ALL_SECTIONS ? {} : { section: search.section },
      order: search.order === "played" ? "mostPlayed" : search.order === "title" ? "title" : search.order === "recent" ? "recent" : "korri"
    };
  }
  function sameQuery(a, b) {
    return a.text === b.text && (a.section ?? "") === (b.section ?? "") && a.order === b.order;
  }
  function detailScreen(ctx, shown) {
    const { korri } = ctx;
    const focused = focusId(ctx);
    const game = picoDetailViewFromGame(shown.game);
    const question = shown.detail.question;
    const focusedGame = korri.focusedGame;
    const actions = focusedGame !== undefined && focusedGame.gameId === shown.game.id ? focusedGame.actions : [];
    const targets = [];
    let shownCard = NO_CARD;
    if (question._tag === "ChoosingLocation") {
      const locations = question.game.locations ?? [];
      const placed = cardTargets(card({ tone: "ask", kicker: "PLAY WHERE?", title: question.game.title, actions: locations.map((location) => button(location.label)) }), locations.map((location) => `location:${location.id}`), locations.map((location) => [nav({ _tag: "ChoseLocation", locationId: location.id })]), focused);
      targets.push(...placed.targets);
      shownCard = placed.card;
    } else if (question._tag === "ConfirmingAction") {
      const placed = cardTargets(card({
        tone: "warn",
        kicker: "ARE YOU SURE?",
        title: `${question.action.label.toUpperCase()}?`,
        message: `${game.title} — this cannot be undone.`,
        actions: [button(question.action.label.toUpperCase(), { warn: true }), button("CANCEL")]
      }), ["confirm", "cancel"], [[nav({ _tag: "ConfirmedGameAction" })], [nav({ _tag: "CancelledGameAction" })]], focused);
      targets.push(...placed.targets);
      shownCard = placed.card;
    } else {
      targets.push({ id: "play", box: { x: 0, y: 0, w: 30, h: 10 }, act: [nav({ _tag: "PressedPlay" })] });
      actions.forEach((action, i) => {
        if (action.enabled)
          targets.push({ id: `action:${action.id}`, box: { x: 0, y: 20 + i * 10, w: 100, h: 8 }, act: [nav({ _tag: "PressedGameAction", action })] });
      });
    }
    const stats = factChips({ ...shown.game, ...game.artUrl === undefined ? {} : { artUrl: game.artUrl } });
    return {
      key: `detail:${shown.game.id}:${question._tag}`,
      fallback: question._tag === "ConfirmingAction" ? "cancel" : "play",
      targets,
      view: {
        page: "detail",
        question: question._tag === "ChoosingLocation" ? "location" : question._tag === "ConfirmingAction" ? "modal" : "",
        frame: frame({ place: "LIBRARY", sep: true, label: game.title, clock: korri.clockLabel ?? "", hints: [A("PLAY"), B("BACK")] }),
        card: shownCard,
        detail: {
          cart: cartView({ ...shown.game, ...game.artUrl === undefined ? {} : { artUrl: game.artUrl } }, { hero: true }),
          title: game.title,
          subtitle: [game.subtitle, game.profile].filter((part) => part !== undefined && part !== "").join(" · "),
          chips: stats,
          primary: button(game.primaryLabel, { focused: focused === "play" }),
          bar: game.downloadProgress === undefined ? NO_BAR : bar(game.downloadProgress, game.download ?? ""),
          actions: actions.map((action) => button(action.label, {
            detail: action.description ?? "",
            disabled: !action.enabled,
            warn: action.destructive === true,
            focused: focused === `action:${action.id}`
          }))
        }
      }
    };
  }
  function cursorGame(focused) {
    if (focused === undefined)
      return;
    for (const prefix of ["cart:", "result:", "resume:"])
      if (focused.startsWith(prefix))
        return focused.slice(prefix.length);
    return;
  }

  // src/native/focus.ts
  var OFF_AXIS_PENALTY = 3;
  function moveFocus(targets, from, direction) {
    const active = targets.find((target) => target.id === from);
    if (active === undefined)
      return targets[0]?.id;
    const named = active.next?.[direction];
    if (named !== undefined && targets.some((target) => target.id === named))
      return named;
    const cx = active.box.x + active.box.w / 2;
    const cy = active.box.y + active.box.h / 2;
    let best;
    for (const candidate of targets) {
      if (candidate === active || candidate.box.w <= 0 || candidate.box.h <= 0)
        continue;
      const dx = candidate.box.x + candidate.box.w / 2 - cx;
      const dy = candidate.box.y + candidate.box.h / 2 - cy;
      const along = direction === "left" ? -dx : direction === "right" ? dx : direction === "up" ? -dy : dy;
      if (along <= 1)
        continue;
      const across = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx);
      const score = along + across * OFF_AXIS_PENALTY;
      if (best === undefined || score < best.score)
        best = { id: candidate.id, score };
    }
    return best?.id;
  }
  function settleFocus(targets, focused, fallback) {
    if (focused !== undefined && targets.some((target) => target.id === focused))
      return focused;
    if (fallback !== undefined && targets.some((target) => target.id === fallback))
      return fallback;
    return targets[0]?.id;
  }

  // src/native/qr.ts
  var QRCore = __toESM(require_qrcode(), 1);
  var create2 = QRCore.create;
  function qrPath(text) {
    try {
      const { modules } = create2(text, { errorCorrectionLevel: "M" });
      const size = modules.size;
      const parts = [];
      for (let y = 0;y < size; y += 1) {
        let x = 0;
        while (x < size) {
          if (!modules.data[y * size + x]) {
            x += 1;
            continue;
          }
          let end = x;
          while (end < size && modules.data[y * size + end])
            end += 1;
          parts.push(`M ${x} ${y} H ${end} V ${y + 1} H ${x} Z`);
          x = end;
        }
      }
      return { path: parts.join(" "), size };
    } catch {
      return;
    }
  }

  // src/native/panel.ts
  var rowLines = (row) => (row.section === "" ? 0 : 1) + (row.source === "" && !row.deviceOnly ? 0 : 1) + (row.description === "" ? 0 : 1) + (row.stateText === "" || row.state === "saving" ? 0 : 1);
  function stacked(rows) {
    let lines = 0;
    return rows.map((row, i) => {
      const placed = { ...row, platesBefore: i, linesBefore: lines };
      lines += rowLines(row);
      return placed;
    });
  }
  var SPINES_BESIDE = 400;
  function rowView(row, focused) {
    const control = row.control;
    const state = row.state;
    const value = control.kind === "step" ? control.current : control.kind === "pick" ? "Choose a setting" : control.kind === "text" && control.sensitive ? row.value ?? "" : control.kind === "network" ? control.network.connected ? "CONNECTED" : "" : row.value ?? "";
    return {
      section: row.section ?? "",
      label: row.label,
      value,
      description: row.description ?? "",
      source: row.source ?? "",
      deviceOnly: row.deviceOnly === true,
      kind: control.kind,
      focused,
      warn: control.kind === "action" && control.destructive,
      state: state === "idle" ? "idle" : state === "saving" ? "saving" : ("problem" in state) ? "problem" : "failed",
      stateText: typeof state === "object" ? "problem" in state ? state.problem : state.failed : state === "saving" ? "SAVING" : "",
      segments: control.kind === "cycle" ? control.options.map((option, i) => ({ label: option.label.toUpperCase(), on: i === control.current })) : [],
      bars: control.kind === "network" ? control.network.bars : 0,
      locked: control.kind === "network" && control.network.secured,
      chips: [],
      platesBefore: 0,
      linesBefore: 0
    };
  }
  function rowAct(row) {
    const control = row.control;
    switch (control.kind) {
      case "fact":
        return;
      case "cycle":
      case "step":
        return [nav({ _tag: "ChangedSetting", settingId: row.id, value: control.next })];
      case "action":
        return control.confirmation !== undefined ? [nav({ _tag: "AskedSettingConfirmation", actionId: control.actionId, confirmation: control.confirmation })] : [nav({ _tag: "PressedSettingAction", actionId: control.actionId })];
      case "text":
        return [nav({ _tag: "OpenedEditor", settingId: row.id })];
      case "pick":
        return [nav({ _tag: "OpenedPicker", settingId: row.id })];
      case "network":
        return [nav({ _tag: "PressedSettingAction", actionId: control.actionId })];
    }
  }
  function panelParts(settings, group, focused, beside, top) {
    const targets = [];
    const at2 = Math.max(0, Math.min(group, settings.groups.length - 1));
    const spines = settings.groups.map((shown, i) => {
      const id = `spine:${i}`;
      targets.push({
        id,
        box: beside ? { x: 0, y: top + i * 10, w: 18, h: 8 } : { x: i * 20, y: 0, w: 18, h: 8 },
        act: [nav({ _tag: "ChoseSettingsGroup", group: i })],
        enter: [nav({ _tag: "ChoseSettingsGroup", group: i })],
        ...beside ? { next: { right: `row:${settings.groups[i]?.rows[0]?.id ?? ""}` } } : {}
      });
      return { label: shown.title, detail: "", focused: focused === id, disabled: false, warn: false, lit: i === at2 };
    });
    const rows = (settings.groups[at2]?.rows ?? []).map((row, i) => {
      const id = `row:${row.id}`;
      const act = rowAct(row);
      targets.push({
        id,
        box: beside ? { x: 20, y: top + i * 10, w: 80, h: 8 } : { x: 0, y: top + 10 + i * 10, w: 100, h: 8 },
        ...act === undefined ? {} : { act },
        ...beside ? { next: { left: `spine:${at2}` } } : {}
      });
      return rowView(row, focused === id);
    });
    return {
      spines,
      spineTone: at2,
      beside,
      rows: stacked(rows),
      focusedRow: Math.max(0, rows.findIndex((row) => row.focused)),
      targets
    };
  }
  var panelView = (parts) => ({
    spines: parts.spines,
    spineTone: parts.spineTone,
    beside: parts.beside,
    rows: parts.rows,
    focusedRow: parts.focusedRow
  });
  var EMPTY_PANEL = {
    spines: [],
    spineTone: 0,
    beside: false,
    rows: [],
    focusedRow: 0,
    scopes: [],
    notes: [],
    message: ""
  };
  function confirmationCard(confirmation, focused) {
    const lines = confirmation.message.split(`
`).filter((line) => line.trim() !== "");
    const report = lines.length > 1;
    return cardTargets(card({
      tone: confirmation.destructive === true ? "warn" : "ask",
      kicker: confirmation.destructive === true ? "ARE YOU SURE?" : "CHECK FIRST",
      title: confirmation.title,
      message: report ? "" : confirmation.message,
      lines: report ? lines.map((text) => ({ text, focused: false })) : [],
      actions: [button(confirmation.confirmLabel.toUpperCase(), { warn: confirmation.destructive === true }), button("CANCEL")],
      inline: report
    }), ["confirm", "cancel"], [[nav({ _tag: "ConfirmedSettingAction" })], [nav({ _tag: "CancelledSettingAction" })]], focused, report ? lines.length : 0);
  }
  var typeInto = (press) => press._tag === "Type" ? [nav({ _tag: "TypedText", character: press.character })] : press._tag === "Backspace" ? [nav({ _tag: "PressedTextBackspace" })] : press._tag === "Clear" ? [nav({ _tag: "ClearedText" })] : press._tag === "Capitals" ? [nav({ _tag: "ToggledCapitals" })] : [nav({ _tag: "ToggledSymbols" })];
  function editorParts(options, focused, press = typeInto) {
    const keys = keyboard(options.charset ?? "text", options.draft, focused, 30, press);
    const targets = [...keys.targets];
    const actions = options.actions.map((action, i) => {
      const id = `answer:${i}`;
      if (action.disabled !== true)
        targets.push({ id, box: { x: i * 50, y: 90, w: 48, h: 8 }, act: action.act });
      return button(action.label, { disabled: action.disabled === true, focused: focused === id });
    });
    if (options.alone !== undefined)
      targets.push({ id: "alone", box: { x: 0, y: 100, w: 100, h: 8 }, act: options.alone.act });
    const text = options.draft.text;
    return {
      editor: {
        group: options.group,
        now: options.now === undefined || options.now === "" ? "" : `NOW ${options.now}`,
        note: options.note ?? "",
        prompt: options.prompt ?? "",
        text: options.secret && options.shown !== true ? "•".repeat([...text].length) : text,
        placeholder: options.placeholder ?? "",
        count: options.maxLength === undefined ? "" : `${[...text].length}/${options.maxLength}`,
        keys: keys.keys,
        keyRows: keys.rows,
        actions,
        alone: button(options.alone?.label ?? "", { warn: true, focused: focused === "alone" }),
        hasAlone: options.alone !== undefined,
        problem: options.problem ?? ""
      },
      targets
    };
  }
  var NO_EDITOR = {
    group: "",
    now: "",
    note: "",
    prompt: "",
    text: "",
    placeholder: "",
    count: "",
    keys: [],
    keyRows: 0,
    actions: [],
    alone: button(""),
    hasAlone: false,
    problem: ""
  };
  function editorNote(row) {
    const numeric = row.control.kind === "text" ? row.control.numeric : undefined;
    const range = numeric === undefined ? undefined : picoNumberRange(numeric);
    return [row.description, range === undefined ? undefined : `${range}.`].filter((part) => part !== undefined).join(" ");
  }
  function question(ctx, settings, layer, title) {
    const focused = ctx.s.focus;
    const asking = layer.question;
    const clock = ctx.korri.clockLabel ?? "";
    if (asking._tag === "ConfirmingAction") {
      const placed = confirmationCard(asking.confirmation, focused);
      return {
        key: `confirm:${asking.actionId}`,
        screen: { targets: placed.targets, fallback: placed.card.lines.length > 0 ? "line:0" : "cancel", view: { card: placed.card } }
      };
    }
    if (asking._tag === "PickingChoice") {
      const row = settings.groups.flatMap((group) => group.rows).find((candidate) => candidate.id === asking.settingId);
      if (row?.control.kind !== "pick")
        return;
      const options = row.control.options;
      const placed = cardTargets(card({ tone: "ask", kicker: "CHOOSE ONE", title: row.label, message: row.description ?? "", actions: [...options.map((option) => button(option.label)), button("CANCEL")] }), [...options.map((option) => `pick:${option.value}`), "cancel"], [...options.map((option) => [nav({ _tag: "PickedChoice", value: option.value })]), [nav({ _tag: "CancelledPicker" })]], focused);
      return { key: `pick:${asking.settingId}`, screen: { targets: placed.targets, view: { card: placed.card } } };
    }
    if (asking._tag === "EditingText" || asking._tag === "ConfirmingClear") {
      const edited = picoTextSettingFrom(settings, asking.settingId);
      if (edited === undefined)
        return;
      const row = edited.row;
      const control = row.control;
      if (asking._tag === "ConfirmingClear") {
        const placed = cardTargets(card({ tone: "warn", kicker: "ARE YOU SURE?", title: `${(control.clearLabel ?? "Clear").toUpperCase()}?`, message: `${row.label} goes back to having no value.`, actions: [button((control.clearLabel ?? "Clear").toUpperCase(), { warn: true }), button("CANCEL")] }), ["confirm", "cancel"], [[nav({ _tag: "ConfirmedClear" })], [nav({ _tag: "CancelledClear" })]], focused);
        return { key: `clear:${row.id}`, screen: { targets: placed.targets, fallback: "cancel", view: { card: placed.card } } };
      }
      const disabled = asking.draft.text.trim() === "" && control.allowEmpty !== true || control.numeric !== undefined && !picoNumberFits(asking.draft.text, control.numeric);
      const range = control.numeric === undefined ? undefined : picoNumberRange(control.numeric);
      const parts = editorParts({
        group: edited.group,
        note: editorNote(row),
        prompt: range === undefined ? "" : `A number, ${range.toLowerCase()}`,
        draft: asking.draft,
        ...control.numeric === undefined && !control.sensitive && row.value !== undefined ? { now: row.value } : {},
        secret: control.sensitive,
        ...control.placeholder === undefined ? {} : { placeholder: control.placeholder },
        ...control.maxLength === undefined ? {} : { maxLength: control.maxLength },
        actions: [
          { label: row.state === "saving" ? "SAVING" : "SAVE", disabled: disabled || row.state === "saving", act: [nav({ _tag: "SavedText" })] },
          { label: "CANCEL", act: [nav({ _tag: "ClosedEditor" })] }
        ],
        ...control.clearLabel === undefined ? {} : { alone: { label: control.clearLabel.toUpperCase(), act: [nav({ _tag: "AskedClear" })] } },
        ...typeof row.state === "object" && "problem" in row.state ? { problem: row.state.problem } : {}
      }, focused);
      return {
        key: `edit:${row.id}`,
        screen: {
          targets: parts.targets,
          fallback: "key:0:0",
          view: {
            page: "editor",
            editor: parts.editor,
            frame: frame({ place: title, label: row.label.toUpperCase(), clock, hints: [A("SELECT"), B("CANCEL")] })
          }
        }
      };
    }
    if (asking._tag === "Wifi") {
      const entry = picoWifiEntryView(asking.entry);
      const parts = editorParts({
        group: "WI-FI",
        note: entry.note ?? "",
        prompt: entry.prompt,
        draft: entry.draft,
        secret: entry.secret,
        shown: entry.shown,
        maxLength: entry.maxLength,
        actions: [
          { label: entry.submitLabel, disabled: !entry.ready, act: [nav({ _tag: "SubmittedWifi" })] },
          { label: "CANCEL", act: [nav({ _tag: "ClosedEditor" })] },
          ...entry.secret ? [{ label: entry.shown ? "HIDE" : "SHOW", act: [nav({ _tag: "ToggledShown" })] }] : []
        ]
      }, focused);
      return {
        key: `wifi:${entry.place}:${entry.prompt}`,
        screen: {
          targets: parts.targets,
          fallback: "key:0:0",
          view: { page: "editor", editor: parts.editor, frame: frame({ place: title, label: entry.place, clock, hints: [A("SELECT"), B("CANCEL")] }) }
        }
      };
    }
    return;
  }
  function settingsScreen(ctx, layer) {
    const { s, korri, model, plan } = ctx;
    const focused = s.focus;
    const settings = picoSettingsViewFromModel(model, { font: s.font });
    if (layer.question._tag === "Identity")
      return identityScreen(ctx, layer.question);
    const asked = question(ctx, settings, layer, "SETTINGS");
    const clock = korri.clockLabel ?? "";
    const base = frame({ label: "SETTINGS", clock, readout: settings.buildLabel ?? "", hints: [A("SELECT"), B("BACK")] });
    if (asked !== undefined && asked.screen.view["page"] === "editor") {
      return { key: `settings:${asked.key}`, targets: asked.screen.targets ?? [], ...asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback }, view: { ...asked.screen.view, card: NO_CARD } };
    }
    if (asked !== undefined) {
      return {
        key: `settings:${asked.key}`,
        targets: asked.screen.targets ?? [],
        ...asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback },
        view: { page: "settings", frame: base, panel: { ...EMPTY_PANEL }, ...asked.screen.view }
      };
    }
    const parts = panelParts(settings, layer.group, focused, plan.width >= SPINES_BESIDE, 0);
    const problem = model.settingsStatus._tag === "Problem" ? model.settingsStatus : undefined;
    const empty = settings.groups.length === 0;
    return {
      key: "settings",
      targets: parts.targets,
      fallback: `spine:${parts.spineTone}`,
      view: {
        page: "settings",
        frame: base,
        card: empty ? card({ tone: "tell", kicker: "NOTHING TO SET", message: "Korri has no settings to show on this device." }) : NO_CARD,
        panel: { ...panelView(parts), scopes: [], notes: [], message: problem === undefined ? "" : problem.message }
      }
    };
  }
  function launchScreen(ctx, launch, layer) {
    const { s, korri, model, plan } = ctx;
    const focused = s.focus;
    const clock = korri.clockLabel ?? "";
    const title = launch.gameTitle ?? "LAUNCH SETTINGS";
    const shown = picoLaunchShown(launch, layer.scope);
    const asked = question(ctx, launch.settings, layer, "LAUNCH SETTINGS");
    const reload = launch.actions.find((action) => action.id.endsWith(":reload") || action.label.toUpperCase() === "RELOAD");
    const leadAct = reload === undefined || !reload.enabled ? undefined : [nav({ _tag: "PressedLaunchAction", actionId: reload.id })];
    const base = frame({
      place: launch.gameTitle ?? "",
      label: "LAUNCH SETTINGS",
      clock,
      hints: [A("SELECT"), B("BACK")],
      hasLead: reload !== undefined,
      leadKind: "plain",
      lead: button((reload?.label ?? "RELOAD").toUpperCase(), { focused: focused === "lead", disabled: reload?.enabled !== true })
    });
    if (asked !== undefined && asked.screen.view["page"] === "editor") {
      return { key: `launch:${asked.key}`, targets: asked.screen.targets ?? [], ...asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback }, view: { ...asked.screen.view, card: NO_CARD } };
    }
    if (asked !== undefined) {
      return {
        key: `launch:${asked.key}`,
        targets: asked.screen.targets ?? [],
        ...asked.screen.fallback === undefined ? {} : { fallback: asked.screen.fallback },
        view: { page: "settings", frame: base, panel: { ...EMPTY_PANEL }, ...asked.screen.view }
      };
    }
    if (launch.phase === "Conflict" || launch.phase === "Error" || launch.phase === "Loading") {
      const answers = launch.actions.filter((action) => action.enabled);
      const placed = cardTargets(launch.phase === "Loading" ? card({ tone: "tell", kicker: "READING", title, message: "Korri is reading this game's launch settings." }) : card({
        tone: launch.phase === "Conflict" ? "ask" : "warn",
        kicker: launch.phase === "Conflict" ? "CHANGED ELSEWHERE" : "COULD NOT READ",
        title,
        message: launch.message ?? "",
        actions: answers.map((action) => button(action.label.toUpperCase()))
      }), answers.map((action) => `launch:${action.id}`), answers.map((action) => [nav({ _tag: "PressedLaunchAction", actionId: action.id })]), focused);
      return {
        key: `launch:${launch.phase}`,
        targets: placed.targets,
        view: { page: "settings", frame: { ...base, hasLead: false }, panel: { ...EMPTY_PANEL }, card: placed.card }
      };
    }
    const beside = plan.width >= SPINES_BESIDE;
    const parts = panelParts(shown, layer.group, focused, beside, 20);
    const current = launch.layers[Math.max(0, Math.min(layer.group, launch.layers.length - 1))];
    const scopeAt = picoLaunchScope(current, layer.scope);
    const scopes = (current?.scopes ?? []).map((scope, i) => {
      const id = `scope:${i}`;
      parts.targets.push({
        id,
        box: beside ? { x: 20 + i * 10, y: 10, w: 9, h: 8 } : { x: i * 10, y: 10, w: 9, h: 8 },
        act: [nav({ _tag: "ChoseLaunchScope", scope: i })],
        enter: [nav({ _tag: "ChoseLaunchScope", scope: i })]
      });
      return button(scope.label.toUpperCase(), { lit: i === scopeAt, focused: focused === id });
    });
    if (leadAct !== undefined)
      parts.targets.push({ id: "lead", box: { x: 0, y: 1000, w: 10, h: 8 }, act: leadAct });
    return {
      key: "launch",
      targets: parts.targets,
      fallback: `row:${shown.groups[parts.spineTone]?.rows[0]?.id ?? ""}`,
      view: {
        page: "settings",
        frame: base,
        card: NO_CARD,
        panel: { ...panelView(parts), scopes, notes: launch.notes, message: "" }
      }
    };
  }
  var mask = (text) => "•".repeat([...text].length);
  function identityScreen(ctx, asking) {
    const { s, korri, model } = ctx;
    const focused = s.focus;
    const clock = korri.clockLabel ?? "";
    if (s.typing !== undefined)
      return typingScreen(ctx, s.typing, "SETTINGS");
    const action = asking.actionId;
    const form = asking.form ?? IDENTITY_FORM;
    const identity = model.identityManagement;
    const status = identity?.status;
    const working = status?._tag === "Working";
    const exportKey = picoIdentityRetiredKey(action, "export");
    const deleteKey = picoIdentityRetiredKey(action, "delete");
    const switching = action === PICO_IDENTITY_SWITCH_LOCAL_ACTION || action === PICO_IDENTITY_SWITCH_NIP46_ACTION;
    const title = action === PICO_IDENTITY_BACKUP_ACTION ? "BACK UP IDENTITY" : action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? "SWITCH FROM BACKUP" : action === PICO_IDENTITY_SWITCH_NIP46_ACTION ? "SWITCH TO NIP-46" : exportKey !== undefined ? "BACK UP RETIRED KEY" : "DELETE RETIRED KEY";
    const rows = [];
    const field = (id, label, value, secret2) => rows.push({
      id,
      view: button(label, { detail: value === "" ? "—" : secret2 ? mask(value) : value.length > 18 ? `${value.slice(0, 18)}…` : value }),
      act: [local((state) => ({ ...state, typing: { field: id, label, draft: { text: value, capitals: false, symbols: false }, secret: secret2 } }))]
    });
    let note = "";
    let qr = "";
    let qrSize = 0;
    let secret = "";
    if (status?._tag === "BackupReady") {
      note = "Korri cannot recover a lost key. If this backup is not exported before device loss, the identity is unrecoverable. Store it outside the device.";
      const drawn = qrPath(status.encryptedSecret);
      qr = drawn?.path ?? "";
      qrSize = drawn?.size ?? 0;
      secret = status.encryptedSecret;
      rows.push({ id: "done", view: button("DONE"), act: [nav({ _tag: "ClosedIdentity" })] });
    } else if (status?._tag === "Switched") {
      note = "Korri is restarting with the new identity.";
      rows.push({ id: "close", view: button("CLOSE"), act: [nav({ _tag: "ClosedIdentity" })] });
    } else {
      if (action === PICO_IDENTITY_BACKUP_ACTION || exportKey !== undefined) {
        field("identity:password", "Password", form.password, true);
        rows.push({
          id: "submit",
          view: button(working && status?._tag === "Working" ? status.operation : "CREATE BACKUP", { disabled: working || form.password.trim() === "" }),
          act: [nav({ _tag: "SubmittedIdentity" })]
        });
      }
      if (action === PICO_IDENTITY_SWITCH_LOCAL_ACTION) {
        field("identity:secret", "Encrypted backup", form.secret, false);
        field("identity:password", "Password", form.password, true);
      }
      if (action === PICO_IDENTITY_SWITCH_NIP46_ACTION)
        field("identity:bunkerUri", "Bunker URI", form.bunkerUri, false);
      if (switching) {
        rows.push({
          id: "disposition",
          view: button("Local records", { detail: form.disposition === "transfer" ? "◀ Transfer ▶" : "◀ Delete ▶" }),
          act: [nav({ _tag: "ChoseIdentityDisposition", disposition: form.disposition === "transfer" ? "delete" : "transfer" })]
        });
        rows.push({
          id: "confirmed",
          view: button(`${form.confirmed ? "[X]" : "[ ]"} I understand trust from the old key is lost`),
          act: [nav({ _tag: "CheckedIdentityConfirmation", confirmed: !form.confirmed })]
        });
        note = "Save files and save states stay under the plugin account.";
        const incomplete = !form.confirmed || (action === PICO_IDENTITY_SWITCH_LOCAL_ACTION ? form.secret.trim() === "" || form.password.trim() === "" : form.bunkerUri.trim() === "");
        rows.push({
          id: "submit",
          view: button(working && status?._tag === "Working" ? status.operation : "SWITCH IDENTITY", { warn: true, disabled: working || incomplete }),
          act: [nav({ _tag: "SubmittedIdentity" })]
        });
      }
      if (deleteKey !== undefined) {
        note = "Delete this private key only after its encrypted backup is stored elsewhere.";
        rows.push({
          id: "confirmed",
          view: button(`${form.confirmed ? "[X]" : "[ ]"} I have stored its backup`),
          act: [nav({ _tag: "CheckedIdentityConfirmation", confirmed: !form.confirmed })]
        });
        rows.push({ id: "submit", view: button("DELETE KEY", { warn: true, disabled: working || !form.confirmed }), act: [nav({ _tag: "SubmittedIdentity" })] });
      }
      rows.push({ id: "close", view: button("CLOSE"), act: [nav({ _tag: "ClosedIdentity" })] });
    }
    const targets = rows.filter((row) => !row.view.disabled).map((row, i) => ({ id: row.id, box: { x: 0, y: i * 10, w: 100, h: 8 }, ...row.act === undefined ? {} : { act: row.act } }));
    return {
      key: `identity:${action}:${status?._tag ?? "Idle"}`,
      targets,
      fallback: rows[0]?.id,
      view: {
        page: "identity",
        frame: frame({ place: "SETTINGS", label: title, clock, hints: [A("SELECT"), B("CLOSE")] }),
        card: NO_CARD,
        identity: {
          title,
          problem: status?._tag === "Problem" ? status.message : "",
          note,
          qr,
          qrSize,
          secret,
          rows: rows.map((row) => ({ ...row.view, focused: focused === row.id }))
        }
      }
    };
  }
  function typingScreen(ctx, typing, place) {
    const focused = ctx.s.focus;
    const edit = (change) => [local((state) => state.typing === undefined ? state : { ...state, typing: { ...state.typing, draft: change(state.typing.draft) } })];
    const parts = editorParts({
      group: typing.label.toUpperCase(),
      draft: typing.draft,
      secret: typing.secret,
      actions: [
        { label: "OK", act: [...commitTyping(typing), local((state) => ({ ...state, typing: undefined }))] },
        { label: "CANCEL", act: [local((state) => ({ ...state, typing: undefined }))] }
      ]
    }, focused, (press) => press._tag === "Type" ? edit((draft) => ({ ...draft, text: draft.text + press.character })) : press._tag === "Backspace" ? edit((draft) => ({ ...draft, text: [...draft.text].slice(0, -1).join("") })) : press._tag === "Clear" ? edit((draft) => ({ ...draft, text: "" })) : press._tag === "Capitals" ? edit((draft) => ({ ...draft, capitals: !draft.capitals })) : edit((draft) => ({ ...draft, symbols: !draft.symbols })));
    return {
      key: `typing:${typing.field}`,
      targets: parts.targets,
      fallback: "key:0:0",
      view: {
        page: "editor",
        card: NO_CARD,
        editor: parts.editor,
        frame: frame({ place, label: typing.label.toUpperCase(), clock: ctx.korri.clockLabel ?? "", hints: [A("SELECT"), B("CANCEL")] })
      }
    };
  }
  function commitTyping(typing) {
    const value = typing.draft.text;
    switch (typing.field) {
      case "identity:password":
        return [nav({ _tag: "EditedIdentity", field: "password", value })];
      case "identity:secret":
        return [nav({ _tag: "EditedIdentity", field: "secret", value })];
      case "identity:bunkerUri":
        return [nav({ _tag: "EditedIdentity", field: "bunkerUri", value })];
      case "backup:password":
        return [{ to: "setup", message: { _tag: "EditedBackupPassword", password: value } }];
    }
  }

  // src/native/sheet.ts
  var item = (kind, label, options = {}) => ({
    kind,
    label,
    value: "",
    focused: false,
    disabled: false,
    warn: false,
    range: false,
    cells: 0,
    arrows: false,
    rowsBefore: 0,
    linesBefore: 0,
    headsBefore: 0,
    ...options
  });
  function finish(items, focused, ids) {
    let rows = 0;
    let lines = 0;
    let heads = 0;
    const marked = items.map((shown, i) => {
      const placed = { ...shown, focused: ids[i] !== undefined && ids[i] === focused, rowsBefore: rows, linesBefore: lines, headsBefore: heads };
      if (shown.kind === "row")
        rows += 1;
      else if (shown.kind === "heading")
        heads += 1;
      else
        lines += 1;
      return placed;
    });
    return { items: marked, focusedItem: Math.max(0, marked.findIndex((shown) => shown.focused)) };
  }
  var overlay = (message) => ({ to: "overlay", message });
  function overlayScreen(view, state, focused) {
    if (state.asking !== undefined) {
      const control2 = state.asking;
      const placed = cardTargets(card({
        tone: "warn",
        kicker: "ARE YOU SURE?",
        title: `${control2.label.toUpperCase()}?`,
        message: control2.description ?? "",
        actions: [button(control2.label.toUpperCase(), { warn: true }), button("CANCEL")]
      }), ["confirm", "cancel"], [[overlay({ _tag: "ConfirmedControl" })], [overlay({ _tag: "CancelledControl" })]], focused);
      return {
        key: `overlay:ask:${control2.id}`,
        targets: placed.targets,
        fallback: "cancel",
        view: { card: placed.card, sheet: emptySheet([A("SELECT"), B("BACK")]) }
      };
    }
    if (view.problem !== undefined) {
      const problem = view.problem;
      const placed = cardTargets(card({ tone: "warn", kicker: problem.kicker, title: view.title, message: problem.reason, actions: problem.canRetry ? [button("RETRY"), button("RESUME")] : [button("RESUME")] }), problem.canRetry ? ["retry", "resume"] : ["resume"], problem.canRetry ? [[overlay({ _tag: "PressedRetry" })], [overlay({ _tag: "PressedBack" })]] : [[overlay({ _tag: "PressedBack" })]], focused);
      return { key: "overlay:problem", targets: placed.targets, view: { card: placed.card, sheet: emptySheet([A("SELECT"), B("RESUME")]) } };
    }
    const items = [];
    const ids = [];
    const targets = [];
    const add = (shown, id) => {
      items.push(shown);
      ids.push(id);
    };
    if (view.streamPlayers !== undefined)
      add(item("text", `Playing: ${view.streamPlayers.join(", ")}`));
    const control = (shown) => {
      const id = `control:${shown.id}`;
      const range = shown.range;
      const value = range === undefined ? undefined : rangeValue(state, shown);
      const cells = range === undefined || value === undefined || range.max <= range.min ? 0 : Math.round((value - range.min) / (range.max - range.min) * 10);
      add(item("row", shown.label, {
        value: range !== undefined && value !== undefined ? picoRangeLabel(value, range) : shown.stateLabel ?? "",
        disabled: !shown.enabled,
        warn: shown.destructive,
        range: range !== undefined,
        cells,
        arrows: shown.stateLabel !== undefined && range === undefined
      }), id);
      if (shown.enabled) {
        const left = range === undefined ? [] : [overlay({ _tag: "SteppedRange", control: shown, request: { way: -1, releaseExpected: false } })];
        const right = range === undefined ? [] : [overlay({ _tag: "SteppedRange", control: shown, request: { way: 1, releaseExpected: false } })];
        targets.push({
          id,
          box: { x: 0, y: targets.length * 10, w: 100, h: 8 },
          ...range === undefined ? { act: [overlay(shown.destructive ? { _tag: "AskedControl", control: shown } : { _tag: "InvokedControl", control: shown })] } : {},
          ...range === undefined ? {} : { horizontal: { left, right } }
        });
      }
      const note = shown.enabled ? shown.description : shown.disabledReason ?? shown.description;
      if (note !== undefined && note !== "")
        add(item("note", note));
    };
    view.controls.forEach(control);
    for (const group of view.groups) {
      add(item("section", group.label));
      group.controls.forEach(control);
    }
    const done = finish(items, focused, ids);
    const ranging = targets.some((target) => target.id === focused && target.horizontal !== undefined);
    return {
      key: "overlay",
      targets,
      view: {
        card: NO_CARD,
        sheet: {
          kicker: "PAUSED",
          title: view.title,
          ...done,
          hints: [...ranging ? [{ key: "lr", label: "ADJUST" }] : [], A("SELECT"), B("RESUME")]
        }
      }
    };
  }
  function emptySheet(hints) {
    return { kicker: "", title: "", items: [], focusedItem: 0, hints };
  }
  function runnerScreen(choice2, focused) {
    const items = [];
    const ids = [];
    const targets = [];
    const add = (shown, id) => {
      items.push(shown);
      ids.push(id);
    };
    const row = (action) => {
      const id = `action:${action.id}`;
      add(item("row", action.label, { disabled: !action.enabled }), id);
      if (action.enabled)
        targets.push({ id, box: { x: 0, y: targets.length * 10, w: 100, h: 8 }, act: [nav({ _tag: "ChoseRunnerAction", actionId: action.id })] });
    };
    add(item("text", choice2.message));
    for (const saved of choice2.saved)
      add(item("text", `${saved.label}: ${saved.runnerId}`));
    for (const warning of new Set(choice2.warnings))
      add(item("text", warning));
    for (const route of choice2.routes) {
      add(item("heading", route.runnerId));
      add(item("fact", "System", { value: route.systemId }));
      add(item("fact", "Family", { value: route.familyId ?? "None" }));
      add(item("fact", "Runner build", { value: route.runnerBuild }));
      add(item("fact", "Program", { value: route.program }));
      for (const warning of new Set(route.warnings))
        add(item("text", warning));
      route.actions.forEach(row);
    }
    choice2.actions.forEach(row);
    const done = finish(items, focused, ids);
    return {
      key: `runner:${choice2._tag}`,
      targets,
      view: {
        page: "runner",
        card: NO_CARD,
        sheet: { kicker: "RUNTIMES", title: choice2.gameTitle, ...done, hints: [A("SELECT"), B("CANCEL")] }
      }
    };
  }

  // src/native/setup.ts
  var EMPTY_PACK = { shown: false, tone: "ask", title: "", message: "", answers: [] };
  function networkRow(network, focused) {
    return rowView({
      id: network.id,
      label: network.ssid,
      control: { kind: "network", actionId: network.id, network },
      state: "idle"
    }, focused);
  }
  var SELECT_BACK = [A("SELECT"), B("BACK")];
  function setupScreen(ctx) {
    const { s, model } = ctx;
    const focused = s.focus;
    const screen = picoOnboardingScreen(s.setup, model);
    const cursor = picoSetupCursor(s.setup.step, screen);
    const track = picoSetupTrack(s.setup, model).map((step4) => ({ state: step4.state }));
    const place = picoSetupPlace(s.setup.step, screen);
    const base = (hints) => frame({ place: "SETUP", label: place, hints, track });
    if (s.typing !== undefined) {
      const shown2 = typingScreen(ctx, s.typing, "SETUP");
      return { ...shown2, view: { ...shown2.view, layer: "setup", frame: { ...shown2.view["frame"], track } } };
    }
    if (screen._tag === "Identity" && screen.backup !== undefined) {
      const backup = screen.backup;
      const shown2 = identityScreen({ ...ctx, model: { ...model, ...backup.identity === undefined ? {} : { identityManagement: backup.identity } } }, { _tag: "Identity", actionId: PICO_IDENTITY_BACKUP_ACTION, form: { ...IDENTITY_FORM, password: backup.password }, qr: undefined });
      const targets2 = shown2.targets.map((target) => target.id === "identity:password" ? { ...target, act: [local((state) => ({ ...state, typing: { field: "backup:password", label: "Password", draft: { text: backup.password, capitals: false, symbols: false }, secret: true } }))] } : target.id === "submit" ? { ...target, act: [setup({ _tag: "SubmittedBackup" })] } : target.id === "close" || target.id === "done" ? { ...target, act: [setup({ _tag: "ClosedBackup" })] } : target);
      return { ...shown2, targets: targets2, view: { ...shown2.view, layer: "setup", frame: base(SELECT_BACK) } };
    }
    if (screen._tag === "Entry") {
      const entry = screen.entry;
      const parts = editorParts({
        group: place,
        note: entry.note ?? "",
        prompt: entry.prompt,
        draft: entry.draft,
        secret: entry.secret,
        shown: entry.shown,
        placeholder: entry.placeholder,
        ...entry.maxLength === undefined ? {} : { maxLength: entry.maxLength },
        actions: [
          { label: entry.busy ? "SAVING" : entry.submitLabel, disabled: !entry.ready || entry.busy, act: [setup({ _tag: "SubmittedText" })] },
          ...entry.secret ? [{ label: entry.shown ? "HIDE" : "SHOW", act: [setup({ _tag: "ToggledShown" })] }] : [],
          ...entry.problem === undefined ? [] : [{ label: "OK", act: [setup({ _tag: "DismissedSettingsProblem" })] }]
        ],
        ...entry.problem === undefined ? {} : { problem: entry.problem }
      }, focused, (press) => press._tag === "Type" ? [setup({ _tag: "TypedText", character: press.character })] : press._tag === "Backspace" ? [setup({ _tag: "PressedTextBackspace" })] : press._tag === "Clear" ? [setup({ _tag: "ClearedText" })] : press._tag === "Capitals" ? [setup({ _tag: "ToggledCapitals" })] : [setup({ _tag: "ToggledSymbols" })]);
      return {
        key: cursor.key,
        targets: parts.targets,
        fallback: "key:0:0",
        view: { layer: "setup", page: "editor", card: NO_CARD, editor: parts.editor, frame: base(SELECT_BACK) }
      };
    }
    const shown = describe(screen);
    const targets = [];
    const rows = shown.networks.map((network, i) => {
      const id = `network:${network.id}`;
      targets.push({ id, box: { x: 0, y: 10 + i * 10, w: 100, h: 8 }, act: [setup({ _tag: "PickedNetwork", networkId: network.id })] });
      return networkRow(network, focused === id);
    });
    const factRows = shown.facts.map((fact) => rowView({ id: fact.label, label: fact.label, value: fact.value, control: { kind: "fact" }, state: "idle" }, false));
    const answers = shown.answers.map((answer, i) => {
      if (answer.disabled !== true) {
        targets.push({
          id: answer.id,
          box: shown.inline ? { x: i * 50, y: 500, w: 48, h: 8 } : { x: 0, y: 500 + i * 10, w: 100, h: 8 },
          act: answer.act ?? (answer.message === undefined ? [] : [setup(answer.message)])
        });
      }
      return button(answer.label, { detail: answer.detail ?? "", disabled: answer.disabled === true, focused: focused === answer.id });
    });
    const pack = shown.pack === undefined ? EMPTY_PACK : {
      ...shown.pack,
      answers: shown.pack.answers.map((answer, i) => {
        if (answer.disabled !== true)
          targets.push({ id: answer.id, box: { x: 0, y: 300 + i * 10, w: 100, h: 8 }, act: answer.message === undefined ? [] : [setup(answer.message)] });
        return button(answer.label, { disabled: answer.disabled === true, focused: focused === answer.id });
      })
    };
    if (shown.go !== undefined && shown.go.disabled !== true) {
      targets.push({ id: shown.go.id, box: { x: 0, y: 700, w: 30, h: 10 }, act: shown.go.message === undefined ? [] : [setup(shown.go.message)] });
    }
    const enabled = targets.map((target) => target.id);
    const fallback = cursor.at === "last" ? enabled[enabled.length - 1] : enabled[0];
    return {
      key: cursor.key,
      targets,
      ...fallback === undefined ? {} : { fallback },
      view: {
        layer: "setup",
        page: "setup",
        card: NO_CARD,
        frame: base(shown.hints),
        setup: {
          step: screen._tag === "Hello" ? "hello" : screen._tag === "Wifi" ? "wifi" : screen._tag === "Identity" ? "you" : screen._tag === "Pads" ? "pads" : screen._tag === "Games" ? "games" : "play",
          layout: shown.layout,
          tone: shown.tone,
          kicker: shown.kicker,
          title: shown.title,
          message: shown.message,
          busy: shown.busy,
          rows: stacked([...rows, ...factRows]),
          carts: shown.carts,
          more: shown.more,
          answers,
          inline: shown.inline,
          go: button(shown.go?.label ?? "", { focused: shown.go !== undefined && focused === shown.go.id, disabled: shown.go?.disabled === true }),
          hasGo: shown.go !== undefined,
          pack
        }
      }
    };
  }
  var card2 = (options) => ({
    layout: "card",
    tone: "ask",
    kicker: "",
    title: "",
    message: "",
    busy: "",
    networks: [],
    facts: [],
    carts: [],
    more: "",
    answers: [],
    inline: false,
    hints: SELECT_BACK,
    ...options
  });
  function describe(screen) {
    switch (screen._tag) {
      case "Hello":
        return card2({
          layout: "hello",
          title: "Hello.",
          message: screen.askWifi ? "Let's get this console ready to play. Have your Wi-Fi password handy." : "Let's get this console ready to play. It takes about a minute.",
          go: { id: "start", label: "START", message: { _tag: "Started" } },
          hints: [A("START")]
        });
      case "Wifi": {
        const view = screen.view;
        const hints = view._tag === "Scanning" || view._tag === "Connecting" ? [B("BACK")] : SELECT_BACK;
        switch (view._tag) {
          case "Scanning":
            return card2({ tone: "tell", title: "Looking for networks", message: "This takes a few seconds.", busy: "Looking for networks", hints });
          case "Networks":
            return card2({
              title: "Pick a network",
              message: view.connectedSsid === undefined ? "Setup needs a connection to go on." : `On ${view.connectedSsid} now.`,
              networks: view.networks,
              inline: true,
              answers: [
                { id: "hidden", label: "Hidden network", message: { _tag: "ChoseHiddenNetwork" } },
                { id: "scan", label: "Scan again", message: { _tag: "ScannedNetworks" } }
              ],
              hints
            });
          case "Connecting":
            return card2({ tone: "tell", title: "Connecting", message: `Joining ${view.ssid}.`, busy: "Connecting", hints });
          case "Connected":
            return card2({
              tone: "tell",
              title: "Connected",
              message: `This device is online through ${view.ssid}.`,
              answers: [
                { id: "next", label: "NEXT", message: { _tag: "Continued" } },
                { id: "change", label: "Change network", message: { _tag: "PickedAnotherNetwork" } }
              ],
              hints
            });
          case "Failed":
            return card2({
              tone: "warn",
              title: view.passwordRejected ? "Wrong password" : "Could not connect",
              message: view.passwordRejected ? `${view.message} Capitals count: turn on SHOW to check it.` : view.message,
              answers: [
                { id: "again", label: "Try again", message: { _tag: "TriedAgain" } },
                { id: "another", label: "Pick another network", message: { _tag: "PickedAnotherNetwork" } },
                view.fallback._tag === "Offline" ? { id: "offline", label: "Continue offline", message: { _tag: "ChoseOffline" } } : { id: "stay", label: view.fallback.label, message: { _tag: "StayedConnected" } }
              ],
              hints
            });
          case "NoNetworks":
            return card2({
              tone: "warn",
              title: "No networks found",
              message: "Move closer to the router and scan again, or join a hidden network by name. A network cable also works.",
              answers: [
                { id: "scan", label: "Scan again", message: { _tag: "ScannedNetworks" } },
                { id: "hidden", label: "Hidden network", detail: "▶", message: { _tag: "ChoseHiddenNetwork" } },
                { id: "offline", label: "Continue offline", message: { _tag: "ChoseOffline" } }
              ],
              hints
            });
          case "Cable":
            return card2({
              tone: "tell",
              title: "Online by cable",
              message: view.wifi ? "This device is online through a network cable. Set up Wi-Fi too if it will leave the cable." : "This device is online through a network cable.",
              answers: [
                { id: "next", label: "NEXT", message: { _tag: "Continued" } },
                ...view.wifi ? [{ id: "wifi", label: "Set up Wi-Fi too", message: { _tag: "ChoseWifiToo" } }] : []
              ],
              hints
            });
          case "NoAdapter":
            return card2({
              tone: "tell",
              title: "No Wi-Fi here",
              message: "This device has no Wi-Fi. Plug in a network cable to go online, or carry on offline: games on this device still play.",
              answers: [{ id: "offline", label: "Continue offline", message: { _tag: "Continued" } }],
              hints
            });
          case "OfflineWarning":
            return card2({
              title: "Continue offline?",
              message: "With no connection, this device cannot restore an identity from a backup or connect a signer. Games on this device still play.",
              answers: [
                { id: "offline", label: "Continue offline", message: { _tag: "ConfirmedOffline" } },
                { id: "back", label: "Back to Wi-Fi", message: { _tag: "ReturnedToWifi" } }
              ],
              hints
            });
        }
        break;
      }
      case "Identity": {
        const view = screen.view;
        const none = view._tag === "Working" || view._tag === "Approving";
        const hints = none ? [] : SELECT_BACK;
        const field = (id, label, value) => ({
          id: `field:${id}`,
          label,
          detail: value === "" ? "EMPTY" : value,
          message: { _tag: "EditedIdentityField", field: id }
        });
        const statement = {
          id: "trust",
          label: "I understand that peer pairings and stream-client trust will be cleared.",
          message: { _tag: "ToggledTrustLoss" }
        };
        switch (view._tag) {
          case "Choose":
            return card2({
              title: "Who are you?",
              message: view.offline ? "Your identity is the key that ties your Korri devices together. With no connection, only a new one can be made." : "Your identity is the key that ties your Korri devices together.",
              answers: [
                { id: "new", label: "New identity", message: { _tag: "ChoseIdentity", how: "created" } },
                { id: "restore", label: "Restore from backup", detail: view.restore.note, disabled: !view.restore.enabled, message: { _tag: "ChoseIdentity", how: "restored" } },
                { id: "signer", label: "Connect a signer", detail: view.signer.note, disabled: !view.signer.enabled, message: { _tag: "ChoseIdentity", how: "signer" } }
              ],
              hints
            });
          case "Working":
            return card2({ tone: "tell", title: "One moment", message: view.label, busy: view.label, hints });
          case "Ready":
            return card2({
              tone: "tell",
              title: view.origin === "created" ? "Identity created" : view.origin === "restored" ? "Identity restored" : "Signer connected",
              message: view.origin === "created" ? `Korri made a key for you on this device. ${view.canBackUp ? "Korri cannot recover a lost key. Back it up now, or later from Settings." : "Korri cannot recover a lost key. Back it up later from Settings."}` : view.origin === "restored" ? "This device uses the identity from your backup." : "Your signer keeps the key. Korri asks it each time it needs to sign.",
              answers: [
                { id: "next", label: "NEXT", message: { _tag: "Continued" } },
                ...view.canBackUp ? [{ id: "backup", label: "Back up now", detail: "▶", message: { _tag: "OpenedBackup" } }] : []
              ],
              hints
            });
          case "Restore":
            return card2({
              title: "Restore from backup",
              message: [view.problem, "Type the encrypted backup and the password it was made with."].filter((part) => part !== undefined).join(" "),
              answers: [
                field("backup", "Backup", view.backup.length > 16 ? `${view.backup.slice(0, 16)}…` : view.backup),
                field("password", "Password", "•".repeat(view.passwordLength)),
                { ...statement, detail: view.confirmed ? "ON" : "OFF" },
                { id: "submit", label: "RESTORE", disabled: !view.ready, message: { _tag: "SubmittedIdentity" } }
              ],
              hints
            });
          case "Signer":
            return card2({
              title: "Connect a signer",
              message: view.problem ?? "Save files and save states stay under the plugin account.",
              answers: [
                field("uri", "Address", view.uri.length > 16 ? `${view.uri.slice(0, 16)}…` : view.uri),
                { ...statement, detail: view.confirmed ? "ON" : "OFF" },
                { id: "submit", label: "CONNECT", disabled: !view.ready, message: { _tag: "SubmittedIdentity" } }
              ],
              hints
            });
          case "Approving":
            return card2({ tone: "tell", title: "Approve in your signer", message: "Your signer app shows a request from this device. Approve it there.", busy: "Waiting for your signer", hints });
        }
        break;
      }
      case "Pads": {
        const view = screen.view;
        const heard = view.controllers.filter((controller) => controller.heard).length;
        const seats = view.seats;
        const refused = seats !== undefined && typeof seats.state === "object";
        return card2({
          tone: view.controllers.length === 0 ? "warn" : "ask",
          title: view.controllers.length === 0 ? "No controllers yet" : heard === 1 ? "1 controller ready" : `${heard} controllers ready`,
          message: view.controllers.length === 0 ? "Korri sees no controllers yet. Connect one now, or later from its own menu." : "Press a button on each controller you will play with. Its lamp lights when Korri hears it.",
          facts: view.controllers.map((controller) => ({ label: controller.name, value: controller.heard ? "READY" : "Press any button" })),
          answers: [
            ...seats === undefined ? [] : [{
              id: "seats",
              label: seats.label,
              detail: seats.control.kind === "step" ? `◀ ${seats.control.current} ▶` : seats.value ?? "",
              ...seats.control.kind === "step" || seats.control.kind === "cycle" ? { message: { _tag: "ChangedSetting", settingId: seats.id, value: seats.control.next } } : {}
            }],
            ...refused ? [{ id: "ok", label: "OK", message: { _tag: "DismissedSettingsProblem" } }] : [],
            { id: "next", label: "NEXT", disabled: !view.ready, message: { _tag: "Continued" } }
          ]
        });
      }
      case "Games": {
        const view = screen.view;
        const pack = screen.starterPack;
        const packCard = pack._tag === "None" ? undefined : {
          shown: true,
          tone: pack._tag === "Failed" ? "warn" : pack._tag === "Installed" ? "tell" : "ask",
          title: pack._tag === "Installing" ? "Installing Starter pack" : pack._tag === "Installed" ? "Starter pack installed" : pack._tag === "Failed" ? "Starter pack not installed" : "Starter pack",
          message: pack._tag === "Offered" ? "Free games and the programs to play them. No internet needed." : pack._tag === "Installing" ? "You can go on. It finishes by itself." : pack._tag === "Installed" ? "Its games are in your library." : pack.message,
          answers: starterPackChoices(pack).map((choice2) => ({ id: `pack:${choice2.id}`, label: choice2.label, disabled: !choice2.enabled, message: { _tag: "ChoseStarterPackAction", actionId: choice2.id } }))
        };
        const scanning = view._tag === "Scanning";
        const hints = scanning && !view.canContinue && (packCard?.answers.length ?? 0) === 0 ? [B("BACK")] : SELECT_BACK;
        const answers = scanning ? view.canContinue ? [{ id: "next", label: "NEXT", message: { _tag: "Continued" } }] : [] : [
          { id: "next", label: "NEXT", message: { _tag: "Continued" } },
          ...view.problem === undefined ? [] : [{ id: "ok", label: "OK", message: { _tag: "DismissedSettingsProblem" } }],
          ...view.canScan ? [{ id: "scan", label: "Scan again", message: { _tag: "ScannedGames" } }] : []
        ];
        const games = view._tag === "Found" ? view.games : [];
        const shownGames = games.slice(0, 8);
        return card2({
          tone: view._tag === "Failed" ? "warn" : scanning ? "tell" : "ask",
          title: scanning ? "Looking for games" : view._tag === "Found" ? `${games.length} games found` : view._tag === "Empty" ? "No games yet" : "Games not read",
          message: scanning ? "Korri is reading this device's games." : view._tag === "Found" ? games.length === 1 ? "Korri found one game to play." : `Korri found ${games.length} games to play.` : view._tag === "Failed" ? view.message : view.problem ?? "",
          busy: scanning ? "Looking for games" : "",
          carts: shownGames.map((game) => cartView(game)),
          more: games.length > shownGames.length ? `+${games.length - shownGames.length} more` : "",
          answers,
          ...packCard === undefined ? {} : { pack: packCard },
          hints
        });
      }
      case "Play":
        return card2({
          layout: "done",
          title: screen.finishing ? "Starting up." : "Ready to play.",
          facts: screen.facts,
          ...screen.finishing ? {} : { go: { id: "finish", label: "GO TO GAMES", message: { _tag: "Finished" } } },
          hints: screen.finishing ? [] : SELECT_BACK
        });
    }
    return card2({});
  }

  // src/native/store.ts
  var actionAct = (action) => [nav({ _tag: "PressedStoreAction", actionId: action.id })];
  function storeScreen(ctx, shown, store) {
    const focused = ctx.s.focus;
    const clock = ctx.korri.clockLabel ?? "";
    const providerTitle = picoStoreProviderTitle(store, shown.providerId);
    const view = shown.view !== "Providers" && providerTitle === undefined && store.providers._tag === "Ready" ? "Providers" : shown.view;
    const details = store.details;
    const label = view === "Providers" || providerTitle === undefined ? "GET GAMES" : providerTitle.toUpperCase();
    const place = view === "Providers" ? "korri" : "get games";
    const targets = [];
    if (shown.typing !== undefined && providerTitle !== undefined) {
      const parts = editorParts({
        group: "SEARCH",
        note: "A name, or part of one",
        prompt: `Search ${providerTitle} for`,
        draft: shown.typing,
        secret: false,
        maxLength: PICO_STORE_QUERY_MAX,
        actions: [
          { label: "SEARCH", act: [nav({ _tag: "SubmittedStoreSearch" })] },
          { label: "CANCEL", act: [nav({ _tag: "ClosedStoreSearch" })] }
        ]
      }, focused);
      return {
        key: "store:typing",
        targets: parts.targets,
        fallback: "key:0:0",
        view: { page: "editor", card: NO_CARD, editor: parts.editor, frame: frame({ place, label, clock, hints: [A("SELECT"), B("CANCEL")] }) }
      };
    }
    const base = (hints2) => frame({ place, label, clock, hints: hints2 });
    const empty = { view: "providers", message: "", providers: [], search: button("SEARCH"), searchNote: "", results: [], focusedResult: 0, entry: emptyEntry(), inline: NO_CARD };
    if (view === "Providers" || view === "Entry" && details._tag === "Closed") {
      const providers = store.providers;
      if (providers._tag !== "Ready") {
        const placed = providers._tag === "Loading" ? { card: card({ tone: "tell", kicker: "LOOKING", message: "Korri is asking which providers it can use." }), targets: [] } : cardTargets(card({ tone: "warn", kicker: "NO PROVIDERS", message: providers.message, actions: providers.actions.map((action) => button(action.label.toUpperCase(), { disabled: !action.enabled })) }), providers.actions.map((action) => `action:${action.id}`), providers.actions.map(actionAct), focused);
        return { key: `store:providers:${providers._tag}`, targets: placed.targets, view: { page: "store", frame: base([B("BACK")]), card: placed.card, store: empty } };
      }
      const rows = providers.providers.map((provider, i) => {
        const id = `provider:${provider.id}`;
        targets.push({ id, box: { x: 0, y: i * 10, w: 100, h: 8 }, act: [nav({ _tag: "ChoseStoreProvider", providerId: provider.id })] });
        return button(provider.title, { focused: focused === id, detail: "▶" });
      });
      return {
        key: "store:providers",
        targets,
        view: {
          page: "store",
          frame: base([A("OPEN"), B("BACK")]),
          card: NO_CARD,
          store: { ...empty, view: "providers", message: rows.length === 0 ? "No provider can get games here yet." : "Pick where to look for a game.", providers: rows }
        }
      };
    }
    if (view === "Results") {
      const search = picoStoreSearchFor(store, shown.providerId);
      targets.push({ id: "search", box: { x: 0, y: 0, w: 100, h: 8 }, act: [nav({ _tag: "OpenedStoreSearch" })] });
      const searchRow = button("SEARCH", { focused: focused === "search", detail: search._tag === "Idle" ? "TYPE A NAME" : search.query });
      if (search._tag === "Problem") {
        const placed = cardTargets(card({ tone: "warn", kicker: "SEARCH FAILED", message: search.message, actions: search.actions.map((action) => button(action.label.toUpperCase(), { disabled: !action.enabled })) }), search.actions.map((action) => `action:${action.id}`), search.actions.map(actionAct), focused);
        targets.push(...placed.targets.map((target) => ({ ...target, box: { ...target.box, y: target.box.y + 20 } })));
        return { key: "store:results:problem", targets, view: { page: "store", frame: base([A("SELECT"), B("BACK")]), card: NO_CARD, store: { ...empty, view: "results", search: searchRow, inline: placed.card } } };
      }
      const entries = search._tag === "Ready" ? search.entries : [];
      const results = entries.map((entry2, i) => {
        const id = `entry:${entry2.id}`;
        targets.push({ id, box: { x: i * 10, y: 20, w: 9, h: 20 }, act: [nav({ _tag: "OpenedStoreEntry", entryId: entry2.id })] });
        const game2 = picoStoreResult(entry2);
        return { cart: cartView(game2, { focused: focused === id, hero: focused === id, band: false }), title: entry2.title, subtitle: game2.subtitle ?? "" };
      });
      const note = search._tag === "Searching" ? "Searching…" : search._tag === "Ready" && entries.length === 0 ? `Nothing matches “${search.query}”.` : search._tag === "Idle" ? "Search for a game by its name." : "";
      return {
        key: "store:results",
        targets,
        fallback: results[0] === undefined ? "search" : `entry:${entries[0]?.id ?? ""}`,
        view: {
          page: "store",
          frame: base([A("OPEN"), B("BACK")]),
          card: NO_CARD,
          store: { ...empty, view: "results", search: searchRow, searchNote: note, results, focusedResult: Math.max(0, results.findIndex((result) => result.cart.focused)), inline: NO_CARD }
        }
      };
    }
    if (details._tag === "Closed")
      return { key: "store:closed", targets: [], view: { page: "store", frame: base([B("BACK")]), card: NO_CARD, store: empty } };
    const entry = details.entry;
    const game = picoStoreResult(entry);
    const cart = cartView(game, { hero: true });
    const entryBase = {
      cart,
      title: entry.title,
      subtitle: [entry.system, providerTitle].filter((part) => part !== undefined && part !== "").join(" · "),
      chips: [chip("NOT ON THIS DEVICE", "empty")],
      description: "",
      hasPrimary: false,
      primary: button(""),
      inline: NO_CARD,
      launch: emptyLaunch(),
      working: false
    };
    if (details._tag === "Loading") {
      return { key: "store:entry:loading", targets: [], view: { page: "store", frame: base([B("BACK")]), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...entryBase, inline: card({ tone: "tell", kicker: "READING", message: "Korri is asking the provider about this game." }) } } } };
    }
    if (details._tag === "Problem") {
      const placed = cardTargets(card({ tone: "warn", kicker: "COULD NOT READ", message: details.message, actions: details.actions.map((action) => button(action.label.toUpperCase(), { disabled: !action.enabled })) }), details.actions.map((action) => `action:${action.id}`), details.actions.map(actionAct), focused);
      return { key: "store:entry:problem", targets: placed.targets, view: { page: "store", frame: base([A("SELECT"), B("BACK")]), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...entryBase, inline: placed.card } } } };
    }
    const get = details.get;
    const described = { ...entryBase, description: details.description ?? "" };
    let inline = NO_CARD;
    let hints = [B("BACK")];
    switch (get._tag) {
      case "Offer": {
        const [first, ...rest] = get.actions;
        if (first !== undefined && first.enabled)
          targets.push({ id: `action:${first.id}`, box: { x: 0, y: 0, w: 30, h: 10 }, act: actionAct(first) });
        rest.forEach((action, i) => {
          if (action.enabled)
            targets.push({ id: `action:${action.id}`, box: { x: 0, y: 20 + i * 10, w: 100, h: 8 }, act: actionAct(action) });
        });
        hints = first === undefined ? [B("BACK")] : [A(first.label.toUpperCase()), B("BACK")];
        return {
          key: "store:entry:offer",
          targets,
          view: {
            page: "store",
            frame: base(hints),
            card: NO_CARD,
            store: {
              ...empty,
              view: "entry",
              entry: {
                ...described,
                description: [get.note, described.description].filter((part) => part !== undefined && part !== "").join(`

`),
                hasPrimary: first !== undefined,
                primary: button((first?.label ?? "").toUpperCase(), { focused: first !== undefined && focused === `action:${first.id}`, disabled: first?.enabled !== true }),
                others: rest.map((action) => button(action.label, { disabled: !action.enabled, focused: focused === `action:${action.id}` }))
              }
            }
          }
        };
      }
      case "Unavailable":
        inline = card({ tone: "tell", kicker: "NOT AVAILABLE", message: get.reason });
        break;
      case "InLibrary": {
        const playable = get.gameId !== undefined;
        const placed = cardTargets(card({ tone: "tell", title: "IN YOUR LIBRARY", message: get.detail ?? "", actions: playable ? [button("▶ PLAY")] : [] }), ["play"], [playable ? [nav({ _tag: "PressedStorePlay", gameId: get.gameId ?? "" })] : undefined], focused);
        targets.push(...placed.targets);
        inline = placed.card;
        hints = playable ? [A("PLAY"), B("BACK")] : [B("BACK")];
        return { key: "store:entry:library", targets, view: { page: "store", frame: base(hints), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...described, chips: [chip("ON THIS DEVICE", "info")], title: "", subtitle: "", inline } } } };
      }
      case "Confirm":
      case "Problem": {
        const placed = cardTargets(card({
          tone: get._tag === "Confirm" ? "ask" : "warn",
          kicker: get.kicker,
          message: get._tag === "Confirm" ? get.detail ?? "" : get.reason,
          actions: get.actions.map((action) => button(action.label.toUpperCase(), { disabled: !action.enabled }))
        }), get.actions.map((action) => `action:${action.id}`), get.actions.map(actionAct), focused);
        targets.push(...placed.targets);
        inline = placed.card;
        hints = [A("SELECT"), B("BACK")];
        break;
      }
      case "Working": {
        get.actions.forEach((action, i) => {
          if (action.enabled)
            targets.push({ id: `action:${action.id}`, box: { x: i * 20, y: 0, w: 18, h: 8 }, act: actionAct(action) });
        });
        const launch = {
          kicker: get.kicker,
          detail: get.detail ?? "",
          bar: get.progress === undefined ? { ...NO_BAR, shown: true, walking: true } : bar(get.progress),
          hasCart: true,
          cart,
          phase: "starting",
          actions: get.actions.map((action) => button(action.label.toUpperCase(), { disabled: !action.enabled, focused: focused === `action:${action.id}` }))
        };
        return { key: "store:entry:working", targets, view: { page: "store", frame: base([B("BACK")]), card: NO_CARD, store: { ...empty, view: "working", entry: { ...described, launch, working: true } } } };
      }
    }
    return { key: `store:entry:${get._tag}`, targets, view: { page: "store", frame: base(hints), card: NO_CARD, store: { ...empty, view: "entry", entry: { ...described, inline } } } };
  }
  function emptyLaunch() {
    return { kicker: "", detail: "", bar: NO_BAR, hasCart: false, cart: cartView({ id: "", title: "" }), phase: "starting", actions: [] };
  }
  function emptyEntry() {
    return {
      cart: cartView({ id: "", title: "" }),
      title: "",
      subtitle: "",
      chips: [],
      description: "",
      hasPrimary: false,
      primary: button(""),
      others: [],
      inline: NO_CARD,
      launch: emptyLaunch(),
      working: false
    };
  }

  // src/native/program.ts
  var WINDOW = 50;
  var EDGE = 8;
  var ATTRACT_AFTER_CLOCK_CHANGES = 2;
  function planFor(korri) {
    const displays = korri.displays ?? [];
    const main = displays.find((display) => display.main) ?? displays[0];
    const width = main?.width ?? 640;
    const height = main?.height ?? 480;
    const p = Math.max(2, Math.floor(Math.min(width, height) / 360));
    return { width: Math.floor(width / p), height: Math.floor(height / p), p };
  }
  function init(korri, preferences) {
    return {
      nav: initialNavigation(),
      setup: initialOnboarding(),
      overlay: initialOverlay,
      font: picoFontFrom(preferences[PICO_FONT_KEY]),
      focus: undefined,
      screen: "",
      overlayFocus: undefined,
      reported: null,
      asked: undefined,
      sections: sectionsOf(korri, []),
      clock: korri.clockLabel,
      quiet: 0,
      typing: undefined,
      noticeRound: 0
    };
  }
  function korriRequest(request, s) {
    switch (request._tag) {
      case "LaunchGame":
        return { _tag: "LaunchGame", gameId: request.gameId, ...request.locationId === undefined ? {} : { launchLocationId: request.locationId } };
      case "DismissProblem":
        return { _tag: "Dismiss" };
      case "ChooseFont":
      case "RememberFont":
        return { _tag: "RememberPreference", key: PICO_FONT_KEY, value: request.font };
      case "RenderQr":
      case "Wait":
        return;
      default:
        return request;
    }
  }
  function deliver(step4, dispatch, korri) {
    const model = picoModelFrom(korri);
    let { state } = step4;
    const requests = [...step4.requests];
    const answer = (picoRequests, to) => {
      for (const request of picoRequests) {
        if (request._tag === "ChooseFont")
          state = { ...state, font: request.font };
        if (request._tag === "RenderQr") {
          const reply = { _tag: "RenderedQr", text: request.text, dataUrl: qrPath(request.text) === undefined ? "" : "drawn" };
          const next = deliver({ state, requests: [] }, to === "setup" ? { to: "setup", message: reply } : { to: "nav", message: reply }, korri);
          state = next.state;
          requests.push(...next.requests);
          continue;
        }
        if (request._tag === "Wait") {
          if (to === "overlay") {
            const next = deliver({ state, requests: [] }, { to: "overlay", message: { _tag: "Waited", token: request.token } }, korri);
            state = next.state;
            requests.push(...next.requests);
          }
          continue;
        }
        const translated = korriRequest(request, state);
        if (translated !== undefined)
          requests.push(translated);
      }
    };
    switch (dispatch.to) {
      case "nav": {
        const result = update(state.nav, dispatch.message, model);
        state = { ...state, nav: result.model };
        answer(result.requests, "nav");
        break;
      }
      case "setup": {
        const result = updateOnboarding(state.setup, dispatch.message, model);
        state = { ...state, setup: result.model };
        answer(result.requests, "setup");
        break;
      }
      case "overlay": {
        if (korri.presentation.kind !== "gameplay-overlay")
          break;
        const view = picoOverlayViewFrom(korri.presentation, korri.status);
        const result = updateOverlay(state.overlay, dispatch.message, view);
        state = { ...state, overlay: result.model };
        answer(result.requests, "overlay");
        break;
      }
      case "local":
        state = dispatch.change(state);
        break;
      case "focus":
        state = { ...state, focus: dispatch.id };
        break;
    }
    return { state, requests };
  }
  var run = (step4, act, korri) => (act ?? []).reduce((acc, dispatch) => deliver(acc, dispatch, korri), step4);
  function catalogScreen(s, korri) {
    const model = picoModelFrom(korri);
    const ctx = { s, korri, model, plan: planFor(korri) };
    if (model.onboarding !== undefined)
      return onPage(setupScreen(ctx), "setup");
    const runner = model.runnerChoice;
    if (runner !== undefined && runner._tag !== "Closed")
      return runnerScreen(runner, s.focus);
    const launch = picoLaunchViewFromModel(model);
    if (launch !== undefined && s.nav.launch !== undefined)
      return onPage(launchScreen(ctx, launch, s.nav.launch), "launch");
    const shown = shownScreen(s.nav, model);
    switch (shown._tag) {
      case "Home":
        return homeScreen(ctx, shown);
      case "Find":
        return findScreen(ctx, shown);
      case "Detail":
        return detailScreen(ctx, shown);
      case "Settings":
        return onPage(settingsScreen(ctx, shown.settings), "settings");
      case "Store":
        return onPage(storeScreen(ctx, shown.place, shown.store), "store");
    }
  }
  function onPage(screen, page) {
    const view = screen.view;
    const raw = view["page"];
    const card3 = view["card"];
    const mode = raw === "editor" ? "editor" : raw === "identity" ? "identity" : card3?.shown === true ? screen.key.includes(":pick:") ? "choice" : "modal" : page === "store" ? "store" : page === "setup" ? "setup" : "panel";
    return { ...screen, view: { ...view, page, mode } };
  }
  function overlayView(s, korri) {
    if (korri.presentation.kind !== "gameplay-overlay")
      return;
    return overlayScreen(picoOverlayViewFrom(korri.presentation, korri.status), s.overlay, s.overlayFocus);
  }
  function settle2(step4, korri) {
    let { state } = step4;
    const requests = [...step4.requests];
    const shown = overlayView(state, korri);
    if (shown !== undefined) {
      const focus = settleFocus(shown.targets, state.overlayFocus, shown.fallback);
      return { state: focus === state.overlayFocus ? state : { ...state, overlayFocus: focus }, requests };
    }
    for (let round = 0;round < 3; round += 1) {
      const screen = catalogScreen(state, korri);
      const changed = screen.key !== state.screen;
      const home = state.nav.home;
      const returning = home.focusOnReturn !== "None" && topLayer(state.nav) === "Home" && screen.key.startsWith("home:");
      const focus = changed || returning ? settleFocus(screen.targets, screen.fallback, undefined) : settleFocus(screen.targets, state.focus, screen.fallback);
      const moved = focus !== state.focus;
      state = { ...state, screen: screen.key, focus };
      const enter = moved ? screen.targets.find((target) => target.id === focus)?.enter : undefined;
      if (enter !== undefined && enter.length > 0) {
        const next = run({ state, requests }, enter, korri);
        state = next.state;
        requests.splice(0, requests.length, ...next.requests);
      }
      if (returning) {
        const next = deliver({ state, requests }, { to: "nav", message: { _tag: "ReturnedFocus" } }, korri);
        state = next.state;
        requests.splice(0, requests.length, ...next.requests);
        continue;
      }
      break;
    }
    return { state, requests };
  }
  function sectionsOf(korri, seen) {
    const catalog = korri.catalog;
    if (catalog._tag !== "Ready" || catalog.query.section != null || catalog.query.text !== "")
      return seen;
    const next = [...seen];
    for (const game of catalog.window)
      if (game.section !== undefined && !next.includes(game.section))
        next.push(game.section);
    return next.length === seen.length ? seen : next;
  }
  var DEFAULT_QUERY = { text: "", order: "korri" };
  function wanted(s, model) {
    const query = s.nav.find ? findQuery(s.nav.search) : DEFAULT_QUERY;
    const cursor = cursorGame(s.focus);
    const anchor = s.nav.detail?.gameId ?? cursor ?? (s.nav.find ? undefined : s.nav.home.selectedGameId);
    return { query, anchor };
  }
  function sync(step4, korri) {
    let { state } = step4;
    const requests = [...step4.requests];
    const model = picoModelFrom(korri);
    if (korri.presentation.kind === "catalog" && model.onboarding === undefined) {
      const { query, anchor } = wanted(state, model);
      const catalog = korri.catalog;
      if (catalog._tag === "Ready") {
        const window = catalog.window;
        const at2 = anchor === undefined ? -1 : window.findIndex((game) => game.id === anchor);
        const sameAsked = sameQuery(catalog.query, query);
        let move;
        if (!sameAsked) {
          move = { _tag: "MoveCatalog", query, from: anchor === undefined || state.nav.find ? { _tag: "Start" } : { _tag: "Around", gameId: anchor }, count: WINDOW };
        } else if (anchor !== undefined && at2 === -1 && cursorGame(state.focus) === undefined) {
          move = { _tag: "MoveCatalog", query, from: { _tag: "Around", gameId: anchor }, count: WINDOW };
        } else if (at2 !== -1 && (catalog.after !== undefined && at2 >= window.length - EDGE || catalog.before !== undefined && at2 < EDGE)) {
          move = { _tag: "MoveCatalog", query, from: { _tag: "Around", gameId: anchor ?? "" }, count: WINDOW };
        }
        if (move !== undefined) {
          const key = JSON.stringify([move, catalog.firstIndex, catalog.total]);
          if (key !== state.asked) {
            requests.push(move);
            state = { ...state, asked: key };
          }
        }
      } else if (catalog._tag !== "Loading" && !sameQuery(DEFAULT_QUERY, query)) {
        const key = JSON.stringify(query);
        if (key !== state.asked) {
          requests.push({ _tag: "MoveCatalog", query, from: { _tag: "Start" }, count: WINDOW });
          state = { ...state, asked: key };
        }
      }
      const reported = state.nav.detail?.gameId ?? cursorGame(state.focus) ?? null;
      if (reported !== state.reported) {
        requests.push(reported === null ? { _tag: "FocusGame" } : { _tag: "FocusGame", gameId: reported });
        state = { ...state, reported };
      }
    }
    return { state, requests };
  }
  function pressed(step4, button2, korri) {
    const s = step4.state;
    if (korri.presentation.kind === "gameplay-overlay") {
      const screen2 = overlayView(s, korri);
      if (screen2 === undefined)
        return step4;
      switch (button2) {
        case "back":
          return run(step4, [{ to: "overlay", message: { _tag: "PressedBack" } }], korri);
        case "menu":
          return run(step4, [{ to: "overlay", message: { _tag: "PressedMenu" } }], korri);
        case "system":
          return run(step4, [{ to: "overlay", message: { _tag: "PressedSystem" } }], korri);
        case "options":
          return step4;
        case "confirm":
          return run(step4, screen2.targets.find((target) => target.id === s.overlayFocus)?.act, korri);
        default: {
          const direction2 = button2;
          const current2 = screen2.targets.find((target) => target.id === s.overlayFocus);
          if ((direction2 === "left" || direction2 === "right") && current2?.horizontal !== undefined) {
            return run(step4, direction2 === "left" ? current2.horizontal.left : current2.horizontal.right, korri);
          }
          const next2 = moveFocus(screen2.targets, s.overlayFocus, direction2);
          if (next2 === undefined || next2 === s.overlayFocus)
            return step4;
          const leaving = current2?.horizontal === undefined || korri.presentation.kind !== "gameplay-overlay" ? [] : leftRange(current2.id, korri);
          return run({ ...step4, state: { ...s, overlayFocus: next2 } }, leaving, korri);
        }
      }
    }
    const model = picoModelFrom(korri);
    if (model.onboarding !== undefined) {
      if (button2 === "back") {
        if (s.typing !== undefined)
          return { ...step4, state: { ...s, typing: undefined } };
        return run(step4, [{ to: "setup", message: { _tag: "PressedBack" } }], korri);
      }
      if (button2 === "options" || button2 === "menu" || button2 === "system")
        return step4;
    } else {
      if (button2 === "back" && s.typing !== undefined)
        return { ...step4, state: { ...s, typing: undefined } };
      const host = {
        back: { _tag: "PressedBack" },
        options: { _tag: "PressedOptions" },
        menu: { _tag: "PressedMenu" },
        system: { _tag: "PressedSystem" }
      };
      const message = host[button2];
      if (message !== undefined)
        return run(step4, [{ to: "nav", message }], korri);
    }
    const screen = catalogScreen(s, korri);
    const waking = model.onboarding === undefined && attractShowing(s.nav, model);
    if (button2 === "confirm") {
      if (waking)
        return run(step4, [{ to: "nav", message: { _tag: "PressedScreen" } }], korri);
      const target = screen.targets.find((candidate) => candidate.id === s.focus);
      const pressedScreen = model.onboarding === undefined ? [{ to: "nav", message: { _tag: "PressedScreen" } }] : [];
      return run(step4, [...pressedScreen, ...target?.act ?? []], korri);
    }
    const direction = button2;
    const current = screen.targets.find((target) => target.id === s.focus);
    const horizontal = direction === "left" || direction === "right" ? current?.horizontal ?? screen.horizontal : undefined;
    if (horizontal !== undefined)
      return run(step4, direction === "left" ? horizontal.left : horizontal.right, korri);
    const next = moveFocus(screen.targets, s.focus, direction);
    const moved = model.onboarding === undefined ? [{ to: "nav", message: { _tag: "MovedFocus" } }] : [];
    if (next === undefined || next === s.focus)
      return run(step4, moved, korri);
    const arrived = screen.targets.find((target) => target.id === next);
    return run({ ...step4, state: { ...s, focus: next } }, [...moved, ...arrived?.enter ?? []], korri);
  }
  function leftRange(id, korri) {
    if (korri.presentation.kind !== "gameplay-overlay")
      return [];
    const view = picoOverlayViewFrom(korri.presentation, korri.status);
    const control = [...view.controls, ...view.groups.flatMap((group) => group.controls)].find((candidate) => `control:${candidate.id}` === id);
    return control === undefined ? [] : [{ to: "overlay", message: { _tag: "LeftRange", control } }];
  }
  function time(step4, korri) {
    const s = step4.state;
    if (korri.clockLabel === s.clock)
      return step4;
    const quiet = s.quiet + 1;
    let next = { ...step4, state: { ...s, clock: korri.clockLabel, quiet } };
    if (quiet >= ATTRACT_AFTER_CLOCK_CHANGES && s.nav.idle._tag === "Awake") {
      next = run(next, [{ to: "nav", message: { _tag: "IdleElapsed" } }], korri);
    }
    return next;
  }
  function update2(state, message, korri) {
    let step4 = { state, requests: [] };
    const model = picoModelFrom(korri);
    switch (message._tag) {
      case "KorriPublished":
        step4 = { ...step4, state: { ...step4.state, sections: sectionsOf(korri, step4.state.sections) } };
        step4 = run(step4, [{ to: "nav", message: { _tag: "KorriPublished" } }], korri);
        if (model.onboarding !== undefined)
          step4 = run(step4, [{ to: "setup", message: { _tag: "KorriPublished" } }], korri);
        if (korri.presentation.kind === "gameplay-overlay")
          step4 = run(step4, [{ to: "overlay", message: { _tag: "KorriPublished" } }], korri);
        step4 = time(step4, korri);
        break;
      case "PreferenceStored":
        break;
      case "Pressed":
        step4 = pressed({ ...step4, state: { ...step4.state, quiet: 0 } }, message.button, korri);
        break;
    }
    const before = new Set(state.nav.invites.showing.map((notice) => notice.inviteId));
    if (step4.state.nav.invites.showing.some((notice) => !before.has(notice.inviteId))) {
      step4 = { ...step4, state: { ...step4.state, noticeRound: step4.state.noticeRound + 1 } };
    }
    step4 = settle2(step4, korri);
    step4 = sync(step4, korri);
    return { state: step4.state, requests: step4.requests };
  }
  var fontFor = (id) => PICO_FONTS.find((font) => font.id === id) ?? PICO_FONTS.find((font) => font.id === PICO_DEFAULT_FONT);
  function view(s, korri) {
    const model = picoModelFrom(korri);
    const screen = catalogScreen(s, korri);
    const font = fontFor(s.font);
    const notices = model.onboarding === undefined ? inviteNotices(s.nav, model) : [];
    const shown = shownScreen(s.nav, model);
    const attract = model.onboarding === undefined && attractShowing(s.nav, model) && shown._tag === "Home" && shown.view._tag === "Shelf";
    const displays = korri.displays ?? [];
    const plan = splitPlan(picoDisplayPlan(displays));
    return {
      fontFamily: font?.family ?? "Pico Tiny5",
      fontRows: font?.rows ?? 7,
      layer: model.onboarding === undefined ? "catalog" : "setup",
      covered: runnerOpen(model) || launchOpen(model),
      ...screen.view,
      notices: notices.map((notice) => ({ playing: `${notice.personName} is playing`, game: notice.gameTitle, where: `on ${notice.deviceLabel}` })),
      noticeRound: s.noticeRound,
      attract: {
        shown: attract,
        carts: attract && shown._tag === "Home" && shown.view._tag === "Shelf" ? shown.view.games.slice(0, 12).map((game) => cartView(game)) : [],
        chosen: shown._tag === "Home" && shown.view._tag === "Shelf" ? chosenIndex(shown.view.games, s.nav.home.selectedGameId) : 0
      },
      split: plan ?? { on: false, showX: 0, showY: 0, showW: 0, showH: 0, chooseX: 0, chooseY: 0, chooseW: 0, chooseH: 0 }
    };
  }
  function splitPlan(plan) {
    if (plan === undefined)
      return;
    const { show, choose } = plan;
    return { on: true, showX: show.x, showY: show.y, showW: show.width, showH: show.height, chooseX: choose.x, chooseY: choose.y, chooseW: choose.width, chooseH: choose.height };
  }
  function overlay2(s, korri) {
    const shown = overlayView(s, korri);
    const font = fontFor(s.font);
    return {
      fontFamily: font?.family ?? "Pico Tiny5",
      fontRows: font?.rows ?? 7,
      ...shown?.view ?? {}
    };
  }

  // logic.ts
  var preferences = {
    [PICO_FONT_KEY]: { mark: "device", initial: PICO_DEFAULT_FONT }
  };
  globalThis.surface = {
    displays: "all",
    preferences,
    init: (model, stored) => init(model, stored),
    update: (state, message, model) => update2(state, message, model),
    view: (state, model) => view(state, model),
    overlay: (state, model) => overlay2(state, model)
  };
})();
