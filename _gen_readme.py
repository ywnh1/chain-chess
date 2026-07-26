import base64,sys
open("/home/ywnh1/Programs/chain-chess/README.md","wb").write(base64.b64decode(sys.argv[1]))
