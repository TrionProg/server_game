About
=====
This is private project, that is why this version is obsolete. License:only reading allowed.

О проекте
=========
Это очень старый репозиторий. Сначала планировалось, что игра не будет работать на кластере. Для каждого юзера есть 2 соединения:
* TCP для передачи файлов и карты
* UDP для передачи событий, что происходит очень часто

Однако в процессе разработки выяснилось, что появляются проблемы с взаимными блокировками. Дело в том, что если в TCP происходит ошибка, нужно сначала заблокировать TCP и удалить его, потом UDP и удалить его,  а что если сломается UDP? Так я пришёл к тому, что сообщения рулят. В современной архитектуре данный узел только помнит и обслуживает клиентов. Получая сообщения от другого узла, он отправлять соответсвующее сообщение по TCP или UDP. Так же UDP должен иметь лёгкий механизм контроля за важными сообщениями(упорядочивание и доставка)

![](https://github.com/TrionProg/server_game/blob/master/arch.jpg)
