
KICAD_PRO:=$(wildcard *.kicad_pro)
PROJECTS=$(KICAD_PRO:%.kicad_pro=%)

$(PROJECTS:%=%.all): %.all: out/%_bom.csv out/%_cpl.csv out/%_gerber.zip out/%.rpt

.PHONY: $(PROJECTS:%=%.all) $(PROJECTS:%=%.drc)

$(PROJECTS:%=%.drc): %.drc: %.rpt
	! grep 'Found [^0]' $<

out/%.rpt: %.kicad_pcb
	mkdir -p out
	kicad-cli pcb drc --exit-code-violations -o $@ $<

$(PROJECTS:%=out/%_bom.csv): %_bom.csv: %_bom_raw.csv filter_pos.py
	./filter_pos.py bom $< $@

$(PROJECTS:%=out/%_bom_raw.csv): out/%_bom_raw.csv: %.kicad_sch *.kicad_sch Parts.kicad_sym
	mkdir -p out
#	kicad-cli sch export bom --exclude-dnp --labels Comment,Designator,Footprint,'JLCPcb Part' --ref-range-delimiter='' -o $@ $<
	kicad-cli sch export bom --preset JLCPCB --exclude-dnp --labels Comment,Designator,Footprint,'JLCPcb Part' --ref-range-delimiter='' -o $@ $<

out/%_gerber.zip: %.kicad_pcb out/%.rpt
	mkdir -p out
	mkdir temp/
	kicad-cli pcb export gerbers --use-drill-file-origin --board-plot-params -o temp $<
	kicad-cli pcb export drill -o temp/ $<
	zip -j $@ temp/*
	rm -r temp

out/%_cpl_all.csv: %.kicad_pcb
	mkdir -p out
	kicad-cli pcb export pos --units mm --format csv --exclude-dnp --side both -o $@ $<

out/%_cpl.csv: out/%_cpl_all.csv out/%_bom.csv filter_pos.py
	mkdir -p out
	./filter_pos.py cpl $< out/$*_bom.csv $@

.PHONY: clean
clean:
	rm -rf out temp
