对于此项目，只维护如下几个页面即可
1. MainPage点击应用后出现的选项页面
2. HistoryPage点击历史记录后出现的页面
3. PvPSetting 主页面点击，本地对战
4. PvESetting 主页面点击人机对战
5. EvESetting 主页面点击 AI 斗蛐蛐
6. GamePage 游戏页面
7. CheckOutPage 历史记录、暂停和结算页面

跳转关系如下
1→2,3,4,5
2→7
3,4,5→6
6→7
7→1,3,4,5
