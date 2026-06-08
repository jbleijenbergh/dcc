# run ollama installation locally , not just docker ollama on windows for amd/nvdia usage


# setup window environment

Op Windows: Zoek in je startmenu naar "Omgevingsvariabelen van het systeem bewerken". 
Voeg bij 'Systeemvariabelen' een nieuwe variabele toe:Naam: 
OLLAMA_MAX_LOADED_MODELS=2
OLLAMA_NUM_PARALLEL=2

Herstart Ollama daarna via het icoontje in de taakbalk


# load models

ollama pull nomic-embed-text


# build local large context model
ollama create qwen3.5-65k -f Dockerfile-qwen3.5-65k
ollama create gemma4-agent -f Dockerfile-gemma4-agent

# setup models for continue

have

```
name: Local Config
version: 1.0.0
schema: v1
models:
  - name: Qwen3.5-65k
    provider: ollama
    model: qwen3.5-65k:latest 
    requestOptions:
      numCtx: 65536
    roles:
      - chat
      - edit
      - apply
    capabilities:
      - tool_use
  - name: Nomic Embed
    provider: ollama
    model: nomic-embed-text:latest
    roles:
      - embed
```

in C:\Users\<xxx>>\.continue\config.yaml

# quality of life

rightclick the continum icon on the left and move it to secondairy bar

# new gemma 4
ollama run gemma4:26b-a4b-it-q4_K_M
