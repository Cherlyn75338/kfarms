VENV:=.venv
PY?=$(VENV)/bin/python
PIP?=$(VENV)/bin/pip

.PHONY: setup test pocs

setup:
	python3 -m venv $(VENV)
	$(PIP) install -r requirements.txt

test:
	PYTHONPATH=. $(PY) -m pytest -q

pocs:
	PYTHONPATH=. $(PY) -m pytest -q pocs -k "not slow"

