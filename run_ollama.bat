# need ollama installation, not just docker ollama on windows for amd/nvdia usage

#then

Op Windows: Zoek in je startmenu naar "Omgevingsvariabelen van het systeem bewerken". 
Voeg bij 'Systeemvariabelen' een nieuwe variabele toe:Naam: 
OLLAMA_MAX_LOADED_MODELS=2
OLLAMA_NUM_PARALLEL=2

Herstart Ollama daarna via het icoontje in de taakbalk


ollama pull nomic-embed-text


# build local large context model
ollama create qwen3.5-65k -f Dockerfile-qwen3.5-65k