# Keep the SM5705 charging at a sensible rate. Ported from the reference port's
# tools/tabs6-charge.sh (BATTERY.md).
#
# The charger powers up with USB safe defaults (500 mA in, 100 mA into the
# battery). On stock the MUIC driver raises them; nothing drives the MUIC here.
# The registers reset when the cable moves, so this loop maintains them.
#
#   VBUSCNTL  0x0D  offset = ((mA - 100) / 25) & 0x7F   max 3275 mA  (input)
#   CHGCNTL2  0x10  offset = ((mA - 100) / 50) & 0x3F   max 3250 mA  (battery)
#
# Overshooting is safe: AICL walks the input back on a weak supply. Nothing is
# touched unless the chip is in charge mode (never while it sources USB_OTG).
BUS=1
ADDR=0x49
REG_CNTL=0x0c
REG_VBUSCNTL=0x0d
REG_CHGCNTL2=0x10
OP_MODE_CHG_ON=5

INPUT_MA=${TABS6_INPUT_MA:-2000}
CHARGE_MA=${TABS6_CHARGE_MA:-2000}
INPUT_OFF=$(( ((INPUT_MA - 100) / 25) & 0x7F ))
CHARGE_OFF=$(( ((CHARGE_MA - 100) / 50) & 0x3F ))

log() { logger -t tabs6-charge -- "$1"; }

log "target ${INPUT_MA}mA input (offset $INPUT_OFF), ${CHARGE_MA}mA charge (offset $CHARGE_OFF)"

while :; do
	mode=$(( $(i2cget -y $BUS $ADDR $REG_CNTL b 2>/dev/null || echo 0) & 7 ))
	if [ "$mode" = "$OP_MODE_CHG_ON" ]; then
		cur_in=$(i2cget -y $BUS $ADDR $REG_VBUSCNTL b 2>/dev/null || echo 0)
		cur_chg=$(i2cget -y $BUS $ADDR $REG_CHGCNTL2 b 2>/dev/null || echo 0)
		# Only ever raise. If something else set a higher limit, leave it.
		if [ $((cur_in)) -lt "$INPUT_OFF" ]; then
			i2cset -y $BUS $ADDR $REG_VBUSCNTL "$INPUT_OFF" b
			log "raised input limit $(( cur_in * 25 + 100 ))mA -> ${INPUT_MA}mA"
		fi
		if [ $((cur_chg)) -lt "$CHARGE_OFF" ]; then
			i2cset -y $BUS $ADDR $REG_CHGCNTL2 "$CHARGE_OFF" b
			log "raised charge current $(( cur_chg * 50 + 100 ))mA -> ${CHARGE_MA}mA"
		fi
	fi
	sleep 10
done
