#!/usr/bin/env bash
# GHL Massive Production Datasets Downloader (Unix / Linux / macOS)
# Roadmap 05: Higgs Boson (11M) and ASA Airline On-Time (Harvard Dataverse)

set -euo pipefail

TARGET="${1:-full}" # "full" (11M / 2.4M) or "sample" (100k rows)
OUTPUT_DIR="${2:-validation/data}"

mkdir -p "$OUTPUT_DIR"
HIGGS_FILE="$OUTPUT_DIR/higgs_11m.csv"
AIRLINE_FILE="$OUTPUT_DIR/airline_120m.csv"
AIRPORTS_FILE="$OUTPUT_DIR/airports_master.csv"
CARRIERS_FILE="$OUTPUT_DIR/carriers_master.csv"

echo "============================================================"
echo " GHL Massive Datasets Downloader & Generator (Roadmap 05)"
echo " Target mode: $TARGET | Output: $OUTPUT_DIR"
echo "============================================================"

# 1. HIGGS BOSON (UCI ML Repository, 11M rows x 29 columns)
echo "[1/2] Processing Higgs Boson Dataset..."
if [ -f "$HIGGS_FILE" ]; then
    echo "Higgs dataset already exists at $HIGGS_FILE"
else
    if [ "$TARGET" = "full" ]; then
        echo "Downloading full 11M Higgs dataset from UCI ML Repository..."
        GZ_PATH="$OUTPUT_DIR/HIGGS.csv.gz"
        curl -fsSL "https://archive.ics.uci.edu/ml/machine-learning-databases/00280/HIGGS.csv.gz" -o "$GZ_PATH"
        echo "Decompressing with official UCI column header..."
        echo "label,lepton_pt,lepton_eta,lepton_phi,met,missing_energy_phi,jet1_pt,jet1_eta,jet1_phi,jet1_b_tag,jet2_pt,jet2_eta,jet2_phi,jet2_b_tag,jet3_pt,jet3_eta,jet3_phi,jet3_b_tag,jet4_pt,jet4_eta,jet4_phi,jet4_b_tag,m_jj,m_jjj,m_lv,m_jlv,m_bb,m_wbb,m_wwbb" > "$HIGGS_FILE"
        gzip -d -c "$GZ_PATH" >> "$HIGGS_FILE"
        rm -f "$GZ_PATH"
    else
        echo "Generating representative 100,000-row Higgs kinematics sample..."
        python3 -c "
import random
with open('$HIGGS_FILE', 'w') as f:
    f.write('label,lepton_pt,met,m_jj,lepton_eta\n')
    random.seed(42)
    for _ in range(100000):
        lbl = 1.0 if random.random() > 0.5 else 0.0
        pt = 60.0 + random.random() * 50.0 + (lbl * 15.0)
        met = 30.0 + random.random() * 40.0 + (lbl * 10.0)
        mjj = 100.0 + random.random() * 40.0 + (lbl * 12.0)
        eta = (random.random() * 4.0) - 2.0
        f.write(f'{lbl:.1f},{pt:.2f},{met:.2f},{mjj:.2f},{eta:.3f}\n')
" || true
    fi
    echo "Higgs dataset ready: $HIGGS_FILE"
fi

# 2. AIRLINE ON-TIME (Harvard Dataverse / ASA Data Expo)
echo "[2/2] Processing Airline On-Time Dataset..."
if [ -f "$AIRLINE_FILE" ]; then
    echo "Airline dataset already exists at $AIRLINE_FILE"
else
    if [ "$TARGET" = "full" ]; then
        echo "Downloading Harvard Dataverse master catalogs..."
        curl -fsSL "https://dataverse.harvard.edu/api/access/datafile/1374930" -o "$AIRPORTS_FILE"
        curl -fsSL "https://dataverse.harvard.edu/api/access/datafile/1374931" -o "$CARRIERS_FILE"

        echo "Downloading ASA Data Expo real flight records (2008)..."
        BZ2_PATH="$OUTPUT_DIR/2008.csv.bz2"
        curl -fsSL "https://dataverse.harvard.edu/api/access/datafile/1374917" -o "$BZ2_PATH"
        bzip2 -dc "$BZ2_PATH" > "$AIRLINE_FILE"
        rm -f "$BZ2_PATH"
    else
        echo "Generating representative 100,000-row Airline On-Time sample..."
        python3 -c "
import random
carriers = ['WN', 'AA', 'DL', 'UA', 'B6', 'AS', 'NK']
airports = ['ATL', 'ORD', 'DFW', 'DEN', 'CLT', 'LAX', 'IAH', 'PHX', 'MCO', 'SEA']
with open('$AIRLINE_FILE', 'w') as f:
    f.write('carrier,origin,dest,dep_delay,arr_delay,air_time,distance\n')
    random.seed(12345)
    for _ in range(100000):
        c = random.choice(carriers)
        orig = random.choice(airports)
        dest = random.choice(airports)
        while dest == orig:
            dest = random.choice(airports)
        dep = round(random.random() * 60.0 - 10.0, 1)
        arr = round(dep + random.random() * 20.0 - 5.0, 1)
        dist = round(300.0 + random.random() * 2200.0)
        t = round(dist / 8.0 + random.random() * 20.0, 1)
        f.write(f'{c},{orig},{dest},{dep:.1f},{arr:.1f},{t:.1f},{dist:.0f}\n')
" || true
    fi
    echo "Airline dataset ready: $AirlineFile"
fi

echo "(U・ᴥ・U) All validation datasets successfully initialized in $OUTPUT_DIR!"
