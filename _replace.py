import sys
c=open(sys.argv[1]).read()
old=sys.argv[2]
new=sys.argv[3]
c=c.replace(old,new)
open(sys.argv[1],"w").write(c)
print("done")
